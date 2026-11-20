//! A minimal Chrome DevTools Protocol session over one flattened target.
//!
//! The image drives its own Chromium: it spawns the binary with `--remote-debugging-port=0`, reads
//! the `DevTools listening on ws://…` line from stderr, and speaks CDP over a client-side
//! websocket. `tokio-tungstenite` with `handshake` is the whole client — no `chromiumoxide`, no
//! Node, no package download. Every call is one request matched by id; event frames are skipped.
//!
//! The session owns **one flattened page target** (`Target.attachToTarget {flatten: true}`), so a
//! command is addressed to that page rather than the browser. Commands that take a selector resolve
//! it with the page's own CSS engine (`DOM.querySelector`); a selector that matches nothing is a
//! named [`CapabilityError::Failed`] for that step, never a silent no-op.
//!
//! Every call is bounded by [`process::TOOL_TIMEOUT`], the same ceiling the image gives any other
//! tool invocation. A wedge therefore fails the step instead of holding the job's deadline.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

use crate::common::{CapabilityError, process};

/// How long a navigation may take to reach `document.readyState === "complete"`. The scenario's
/// own ceiling is the outer bound; this one turns an unloadable page into a step failure sooner.
const LOAD_TIMEOUT: Duration = Duration::from_secs(15);

/// How often the load poll asks the page whether it is still loading. Short enough that a local
/// page costs one extra round trip, long enough that a slow page is not polled in a spin.
const READY_POLL: Duration = Duration::from_millis(20);

/// Where the mouse wheel is aimed when the viewport cannot be measured: the middle of the
/// image's default window (`--window-size=1280,720`), so the event lands on the page either way.
const VIEWPORT_FALLBACK: (f64, f64) = (639.0, 359.0);

/// One attached page target and its websocket.
pub struct Session {
    socket: WebSocketStream<TcpStream>,
    /// The flattened session id; empty until [`Session::launch`] has attached its target.
    session_id: String,
    next_id: u64,
    /// Held for the session's life: see [`LaunchLock`].
    _lock: LaunchLock,
}

/// A machine-wide "one browser at a time" lock.
///
/// The image runs one instruction per pod, so in production this never contends. `nextest` runs
/// each test in its own process, though, and the per-process guard the screenshot path uses cannot
/// serialise two of them: measured on CI 2026-11-16, four Gherkin tests launching Chromium at once
/// each hit their 20 s ceiling at the first step, while the same tests passed in 2 s when they did
/// not collide. A lock file is the cheapest cross-process answer, and it keeps the tests honest —
/// they still drive a real browser, just not four at the same instant.
pub(crate) struct LaunchLock {
    path: PathBuf,
}

impl LaunchLock {
    async fn acquire() -> Result<Self, CapabilityError> {
        let path = std::env::temp_dir().join("conveyor-cdp-launch.lock");
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    // A launcher that died must not wedge the machine: a lock older than two
                    // minutes belongs to nobody.
                    let stale = std::fs::metadata(&path)
                        .and_then(|meta| meta.modified())
                        .ok()
                        .and_then(|modified| modified.elapsed().ok())
                        .is_some_and(|age| age > Duration::from_secs(120));
                    if stale {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    if Instant::now() >= deadline {
                        return Err(CapabilityError::Failed(
                            "timed out waiting for the browser launch lock".to_string(),
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => {
                    return Err(CapabilityError::Failed(format!(
                        "cannot create the browser launch lock: {error}"
                    )));
                }
            }
        }
    }
}

impl Drop for LaunchLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Session {
    /// Spawn `binary` headless with a fresh `profile`, wait for its DevTools endpoint, connect, and
    /// attach one page target.
    ///
    /// The child is returned alongside the session because it is the caller's lifetime: this type
    /// never kills a process it did not return. `kill_on_drop` is set on the child, so dropping it
    /// ends the browser.
    pub async fn launch(
        binary: &str,
        profile: &Path,
        headless_args: &[String],
    ) -> Result<(Child, Session), CapabilityError> {
        let mut args: Vec<String> = vec![
            "--headless=new".to_string(),
            "--disable-gpu".to_string(),
            "--no-sandbox".to_string(),
            "--no-first-run".to_string(),
            "--disable-extensions".to_string(),
            "--disable-dev-shm-usage".to_string(),
            format!("--user-data-dir={}", profile.display()),
            "--remote-debugging-port=0".to_string(),
        ];
        args.extend(headless_args.iter().cloned());
        // Serialised across processes: see [`LaunchLock`]. Acquired before the spawn so two
        // launches never race for the machine's process table, released when the session ends.
        let lock = LaunchLock::acquire().await?;
        // `--remote-debugging-port` alone starts no page; the positional URL is the page the
        // session attaches to before the scenario navigates it.
        args.push("about:blank".to_string());

        let mut child = Command::new(binary)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| CapabilityError::ToolMissing {
                tool: binary.to_string(),
                source,
            })?;

        match attach(&mut child, lock).await {
            Ok(session) => Ok((child, session)),
            Err(error) => {
                // A browser that printed no endpoint is still running; own its death here.
                let _ = child.start_kill();
                Err(error)
            }
        }
    }

    /// One request/response over the session. Event frames (no `id`) and frames for another id are
    /// skipped; a CDP `error` object becomes [`CapabilityError::Failed`] naming its code.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, CapabilityError> {
        let id = self.next_id;
        self.next_id += 1;
        let mut request = json!({"id": id, "method": method, "params": params});
        if !self.session_id.is_empty() {
            request["sessionId"] = Value::String(self.session_id.clone());
        }
        self.socket
            .send(Message::Text(request.to_string().into()))
            .await
            .map_err(|error| {
                CapabilityError::Failed(format!("cdp {method}: cannot send the request: {error}"))
            })?;

        loop {
            let frame = match tokio::time::timeout(process::TOOL_TIMEOUT, self.socket.next()).await
            {
                Err(_) => {
                    return Err(CapabilityError::Failed(format!(
                        "cdp {method}: no response within {} s",
                        process::TOOL_TIMEOUT.as_secs()
                    )));
                }
                Ok(None) => {
                    return Err(CapabilityError::Failed(format!(
                        "cdp {method}: the browser closed the connection"
                    )));
                }
                Ok(Some(Err(error))) => {
                    return Err(CapabilityError::Failed(format!(
                        "cdp {method}: websocket error: {error}"
                    )));
                }
                Ok(Some(Ok(frame))) => frame,
            };
            match frame {
                Message::Text(text) => {
                    let Ok(value) = serde_json::from_str::<Value>(&text) else {
                        continue;
                    };
                    if value.get("id").and_then(Value::as_u64) != Some(id) {
                        continue;
                    }
                    if let Some(error) = value.get("error") {
                        let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
                        let message = error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("no message");
                        return Err(CapabilityError::Failed(format!(
                            "cdp {method} failed ({code}): {message}"
                        )));
                    }
                    return Ok(value.get("result").cloned().unwrap_or(Value::Null));
                }
                Message::Ping(payload) => {
                    self.socket
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|error| {
                            CapabilityError::Failed(format!("cdp {method}: cannot pong: {error}"))
                        })?;
                }
                Message::Close(_) => {
                    return Err(CapabilityError::Failed(format!(
                        "cdp {method}: the browser closed the connection"
                    )));
                }
                Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => {}
            }
        }
    }

    /// Load `url` and wait for the page to report `document.readyState === "complete"`.
    pub async fn navigate(&mut self, url: &str) -> Result<(), CapabilityError> {
        let result = self
            .call("Page.navigate", json!({"url": url}))
            .await
            .map_err(|error| naming("navigate", error))?;
        if let Some(error) = result.get("errorText").and_then(Value::as_str) {
            return Err(step_failed(
                "navigate",
                format!("the browser could not load {url:?}: {error}"),
            ));
        }
        self.wait_ready(url).await?;
        self.settle("navigate").await
    }

    /// Click the element `selector` resolves to: a real mouse move, press and release at its
    /// centre, after scrolling it into view.
    pub async fn click(&mut self, selector: &str) -> Result<(), CapabilityError> {
        let node = self.node_for("click", selector).await?;
        self.scroll_into_view("click", node).await?;
        let (x, y) = self.center_of("click", node).await?;
        self.dispatch("click", "mouseMoved", x, y, "none", 0)
            .await?;
        self.dispatch("click", "mousePressed", x, y, "left", 1)
            .await?;
        self.dispatch("click", "mouseReleased", x, y, "left", 0)
            .await?;
        self.settle("click").await
    }

    /// Type `text` into the element `selector` resolves to: focus it, then insert the text as the
    /// page's own input event. Existing content is kept, as typing would.
    pub async fn type_text(&mut self, selector: &str, text: &str) -> Result<(), CapabilityError> {
        let node = self.node_for("type", selector).await?;
        self.scroll_into_view("type", node).await?;
        self.call("DOM.focus", json!({"nodeId": node}))
            .await
            .map_err(|error| naming("type", error))?;
        self.call("Input.insertText", json!({"text": text}))
            .await
            .map_err(|error| naming("type", error))?;
        self.settle("type").await
    }

    /// Scroll the page by `dy` CSS pixels (positive is down) with a real wheel event.
    pub async fn scroll(&mut self, dy: i64) -> Result<(), CapabilityError> {
        let (x, y) = self.viewport_center().await?;
        self.call(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseWheel",
                "x": x,
                "y": y,
                "deltaX": 0,
                "deltaY": dy,
            }),
        )
        .await
        .map_err(|error| naming("scroll", error))?;
        self.settle("scroll").await
    }

    /// Move the pointer to the centre of the element `selector` resolves to.
    pub async fn hover(&mut self, selector: &str) -> Result<(), CapabilityError> {
        let node = self.node_for("hover", selector).await?;
        self.scroll_into_view("hover", node).await?;
        let (x, y) = self.center_of("hover", node).await?;
        self.dispatch("hover", "mouseMoved", x, y, "none", 0)
            .await?;
        self.settle("hover").await
    }

    /// Set a file input's files, the browser's own `DOM.setFileInputFiles` path, which fires the
    /// page's `input` and `change` events.
    pub async fn upload(&mut self, selector: &str, path: &Path) -> Result<(), CapabilityError> {
        let node = self.node_for("upload", selector).await?;
        let file = path.to_string_lossy().into_owned();
        self.call(
            "DOM.setFileInputFiles",
            json!({"files": [file], "nodeId": node}),
        )
        .await
        .map_err(|error| naming("upload", error))?;
        self.settle("upload").await
    }

    /// The live document, after every step that ran before this one.
    pub async fn dump_dom(&mut self) -> Result<String, CapabilityError> {
        let value = self
            .evaluate("document.documentElement.outerHTML", "assertion")
            .await?;
        value
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| step_failed("assertion", "the browser returned no document"))
    }

    /// Capture the page as a PNG at `path`.
    pub async fn screenshot(&mut self, path: &Path) -> Result<(), CapabilityError> {
        let capture = self
            .call("Page.captureScreenshot", json!({"format": "png"}))
            .await
            .map_err(|error| naming("screenshot", error))?;
        let data = capture
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| step_failed("screenshot", "the browser returned no image data"))?;
        let bytes = base64_decode(data).map_err(|error| step_failed("screenshot", error))?;
        if bytes.is_empty() {
            return Err(step_failed(
                "screenshot",
                "the browser returned an empty image",
            ));
        }
        std::fs::write(path, bytes)
            .map_err(|error| step_failed("screenshot", format!("cannot write {path:?}: {error}")))
    }

    /// Resolve `selector` to a DOM node, or fail the step by name.
    async fn node_for(&mut self, step: &str, selector: &str) -> Result<u64, CapabilityError> {
        let document = self
            .call("DOM.getDocument", json!({"depth": 0}))
            .await
            .map_err(|error| naming(step, error))?;
        let root = document
            .pointer("/root/nodeId")
            .and_then(Value::as_u64)
            .ok_or_else(|| step_failed(step, "the browser returned no document node"))?;
        let found = self
            .call(
                "DOM.querySelector",
                json!({"nodeId": root, "selector": selector}),
            )
            .await
            .map_err(|error| naming(step, error))?;
        match found.get("nodeId").and_then(Value::as_u64) {
            Some(node) if node != 0 => Ok(node),
            _ => Err(step_failed(
                step,
                format!("no element matches the selector {selector:?}"),
            )),
        }
    }

    async fn scroll_into_view(&mut self, step: &str, node: u64) -> Result<(), CapabilityError> {
        self.call("DOM.scrollIntoViewIfNeeded", json!({"nodeId": node}))
            .await
            .map_err(|error| naming(step, error))?;
        Ok(())
    }

    /// The centre of a node's content box, in viewport CSS pixels.
    async fn center_of(&mut self, step: &str, node: u64) -> Result<(f64, f64), CapabilityError> {
        let model = self
            .call("DOM.getBoxModel", json!({"nodeId": node}))
            .await
            .map_err(|error| naming(step, error))?;
        let quad = model
            .pointer("/model/content")
            .and_then(Value::as_array)
            .ok_or_else(|| step_failed(step, "the element has no layout box to aim at"))?;
        let points: Vec<f64> = quad.iter().filter_map(Value::as_f64).collect();
        if points.len() < 8 {
            return Err(step_failed(step, "the element's layout box is incomplete"));
        }
        Ok((
            (points[0] + points[2] + points[4] + points[6]) / 4.0,
            (points[1] + points[3] + points[5] + points[7]) / 4.0,
        ))
    }

    async fn dispatch(
        &mut self,
        step: &str,
        kind: &str,
        x: f64,
        y: f64,
        button: &str,
        buttons: i64,
    ) -> Result<(), CapabilityError> {
        self.call(
            "Input.dispatchMouseEvent",
            json!({
                "type": kind,
                "x": x,
                "y": y,
                "button": button,
                "buttons": buttons,
                "clickCount": if kind == "mouseMoved" { 0 } else { 1 },
            }),
        )
        .await
        .map_err(|error| naming(step, error))?;
        Ok(())
    }

    async fn viewport_center(&mut self) -> Result<(f64, f64), CapabilityError> {
        let value = self
            .evaluate(
                "({ x: Math.floor(window.innerWidth / 2), y: Math.floor(window.innerHeight / 2) })",
                "scroll",
            )
            .await?;
        let x = value
            .get("x")
            .and_then(Value::as_f64)
            .unwrap_or(VIEWPORT_FALLBACK.0);
        let y = value
            .get("y")
            .and_then(Value::as_f64)
            .unwrap_or(VIEWPORT_FALLBACK.1);
        Ok((x, y))
    }

    /// Let the page's own listeners run before the next command reads the DOM: two animation
    /// frames, which is where a click handler's DOM write and a scroll event land.
    async fn settle(&mut self, step: &str) -> Result<(), CapabilityError> {
        let expression = "new Promise((resolve) => requestAnimationFrame(() => \
                          requestAnimationFrame(() => resolve(true))))";
        self.evaluate(expression, step).await.map(|_| ())
    }

    /// `Runtime.evaluate` of one expression by value, with the promise awaited.
    async fn evaluate(&mut self, expression: &str, step: &str) -> Result<Value, CapabilityError> {
        let result = self
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
            )
            .await
            .map_err(|error| naming(step, error))?;
        if let Some(details) = result.get("exceptionDetails") {
            let text = details
                .get("exception")
                .and_then(|exception| exception.get("description"))
                .and_then(Value::as_str)
                .or_else(|| details.get("text").and_then(Value::as_str))
                .unwrap_or("no exception text");
            return Err(step_failed(
                step,
                format!("the page raised an exception: {text}"),
            ));
        }
        Ok(result
            .get("result")
            .and_then(|result| result.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// Poll the page until the navigation has committed and finished loading.
    async fn wait_ready(&mut self, url: &str) -> Result<(), CapabilityError> {
        let started = std::time::Instant::now();
        loop {
            // A navigation in flight destroys the current execution context, so an evaluate that
            // fails is the "not committed yet" answer, not a verdict.
            if let Ok(state) = self
                .evaluate(
                    "({ href: location.href, ready: document.readyState })",
                    "navigate",
                )
                .await
            {
                let href = state.get("href").and_then(Value::as_str).unwrap_or("");
                let ready = state.get("ready").and_then(Value::as_str).unwrap_or("");
                if ready == "complete" && href != "about:blank" {
                    return Ok(());
                }
            }
            if started.elapsed() >= LOAD_TIMEOUT {
                return Err(step_failed(
                    "navigate",
                    format!(
                        "the page did not finish loading {url:?} within {} s",
                        LOAD_TIMEOUT.as_secs()
                    ),
                ));
            }
            tokio::time::sleep(READY_POLL).await;
        }
    }
}

/// Read chromium's stderr up to the DevTools endpoint, connect, and attach one flattened page.
async fn attach(child: &mut Child, lock: LaunchLock) -> Result<Session, CapabilityError> {
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CapabilityError::Failed("chromium's stderr was not captured".to_string()))?;
    let mut lines = BufReader::new(stderr).lines();
    let mut tail: Vec<String> = Vec::new();

    let endpoint = loop {
        match tokio::time::timeout(process::TOOL_TIMEOUT, lines.next_line()).await {
            Err(_) => {
                return Err(CapabilityError::Failed(format!(
                    "chromium printed no DevTools endpoint within {} s: {}",
                    process::TOOL_TIMEOUT.as_secs(),
                    tail.join("\n")
                )));
            }
            Ok(Err(error)) => {
                return Err(CapabilityError::Failed(format!(
                    "cannot read chromium's stderr: {error}"
                )));
            }
            Ok(Ok(None)) => {
                return Err(CapabilityError::Failed(format!(
                    "chromium exited before printing its DevTools endpoint: {}",
                    tail.join("\n")
                )));
            }
            Ok(Ok(Some(line))) => {
                if let Some(endpoint) = line.strip_prefix("DevTools listening on ") {
                    let endpoint = endpoint.trim().to_string();
                    // Keep draining: chrome logs steadily and a full pipe would wedge it.
                    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
                    break endpoint;
                }
                tail.push(line);
                if tail.len() > 8 {
                    tail.remove(0);
                }
            }
        }
    };

    let authority = endpoint
        .strip_prefix("ws://")
        .and_then(|rest| rest.split('/').next())
        .filter(|authority| !authority.is_empty())
        .ok_or_else(|| {
            CapabilityError::Failed(format!("unexpected DevTools endpoint {endpoint:?}"))
        })?;
    let stream = TcpStream::connect(authority).await.map_err(|error| {
        CapabilityError::Failed(format!(
            "cannot reach chromium's DevTools endpoint {endpoint:?}: {error}"
        ))
    })?;
    let (socket, _response) = tokio_tungstenite::client_async(endpoint.as_str(), stream)
        .await
        .map_err(|error| {
            CapabilityError::Failed(format!(
                "cannot open a CDP websocket to {endpoint:?}: {error}"
            ))
        })?;

    let mut session = Session {
        socket,
        session_id: String::new(),
        next_id: 1,
        _lock: lock,
    };
    let created = session
        .call("Target.createTarget", json!({"url": "about:blank"}))
        .await?;
    let target = created
        .get("targetId")
        .and_then(Value::as_str)
        .ok_or_else(|| CapabilityError::Failed("Target.createTarget returned no target".into()))?
        .to_string();
    let attached = session
        .call(
            "Target.attachToTarget",
            json!({"targetId": target, "flatten": true}),
        )
        .await?;
    session.session_id = attached
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| CapabilityError::Failed("Target.attachToTarget returned no session".into()))?
        .to_string();
    for domain in ["Page", "DOM", "Runtime"] {
        session.call(&format!("{domain}.enable"), json!({})).await?;
    }
    Ok(session)
}

/// A failure that names the step it belongs to.
fn step_failed(step: &str, message: impl std::fmt::Display) -> CapabilityError {
    CapabilityError::Failed(format!("{step}: {message}"))
}

/// Prefix an inner CDP failure with the step, leaving a non-`Failed` error untouched.
fn naming(step: &str, error: CapabilityError) -> CapabilityError {
    match error {
        CapabilityError::Failed(message) => CapabilityError::Failed(format!("{step}: {message}")),
        other => other,
    }
}

/// A hand-written base64 decoder for `Page.captureScreenshot`'s `data` field. The alphabet is the
/// standard one; padding and whitespace are ignored.
fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    for byte in text.bytes() {
        if byte == b'=' || byte.is_ascii_whitespace() {
            continue;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            other => return Err(format!("not base64: unexpected byte {other:#04x}")),
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
            // Keep only the bits that have not been emitted yet, so the shift cannot overflow.
            accumulator &= (1u32 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::base64_decode;

    #[test]
    fn the_decoder_reads_the_screenshot_alphabet() {
        assert_eq!(base64_decode("aGVsbG8=").expect("valid"), b"hello");
        assert_eq!(base64_decode("").expect("empty"), b"");
        assert_eq!(
            base64_decode("AP+A").expect("all bytes"),
            [0x00, 0xff, 0x80]
        );
        assert_eq!(base64_decode("aGVs\nbG8=").expect("wrapped"), b"hello");
        assert!(base64_decode("not*base64").is_err());
    }
}
