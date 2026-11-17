//! An ephemeral Redis for the bus tests.
//!
//! The same ladder as the Postgres half of this module, for the same reason (CONTRACT §10): a
//! `TEST_REDIS_URL` the caller already runs wins, then a `REDIS_URL`, then a `docker run` on an
//! OS-assigned port that is removed at teardown. A container that cannot start **fails** the
//! test — nothing here skips, because a bus test that skips proves nothing about the bus.
//!
//! One Redis serves every test in a binary. The bus tests take their own tenant, so `stream_for`
//! gives them disjoint streams — but a tenant does not survive `FLUSHALL`, which the replay test
//! issues on purpose. `crates/actions/tests/bus_delivery.rs` is therefore serialised by the
//! `serial-redis-loss` test group in `.config/nextest.toml`; any binary that shares this Redis and
//! wipes it needs the same treatment.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use conveyor_bus::Bus;
use tokio::sync::OnceCell;
use uuid::Uuid;

/// The image used when `TEST_REDIS_IMAGE` is unset, pinned exactly as `postgres:18` is: a moving
/// tag would turn a changed stream reply into "a version difference" instead of a bug.
const IMAGE: &str = "library/redis:8.10.1-alpine";

static URL: OnceCell<String> = OnceCell::const_new();

/// The Redis URL, starting a container on first use.
pub async fn url() -> &'static str {
    URL.get_or_init(|| async {
        if let Ok(url) = std::env::var("TEST_REDIS_URL")
            && !url.trim().is_empty()
        {
            return url;
        }
        if let Ok(url) = std::env::var("REDIS_URL")
            && !url.trim().is_empty()
        {
            return url;
        }
        start_container().await
    })
    .await
}

/// A connected [`Bus`] ([`url`] starts the container on first use).
pub async fn bus() -> Bus {
    Bus::connect(url().await)
        .await
        .expect("connect to the test Redis")
}

/// A Redis URL nobody else is using — for the one scenario that wipes the server.
///
/// `flush_all` is `FLUSHALL`: server-wide, not database-scoped. On the shared test Redis it deletes
/// every sibling test's streams and consumer groups mid-run; measured 2026-11-15, that is what
/// failed CI with `NOGROUP` inside `a_large_job_round_trips_intact` while the flush test itself
/// passed. This deliberately never consults `TEST_REDIS_URL`/`REDIS_URL`: a caller-provided URL is
/// exactly the shared server the flush test must not touch.
///
/// A runner may instead provide a **second, private** server through `TEST_REDIS_PRIVATE_URL` (the
/// coding gate's kata Pod starts one on another port and exports it). That URL wins and no
/// container is started — the kata Pod has no docker.
pub async fn private_url() -> String {
    private_url_with(|key| std::env::var(key).ok(), start_container()).await
}

/// The decision [`private_url`] makes, with both dependencies injected.
///
/// `start` is a future, not a call: the environment branch must short-circuit before a container
/// is ever started. The test below passes a ready future and asserts the environment URL wins, so
/// deleting the branch fails the test instead of leaving it green while the kata Pod (which has no
/// docker) panics in `start_container`.
async fn private_url_with(
    get: impl Fn(&str) -> Option<String>,
    start: impl std::future::Future<Output = String>,
) -> String {
    if let Some(url) = private_url_from(get) {
        return url;
    }
    start.await
}

/// The private URL the environment names, if any. Pure, so a test can prove the decision without
/// touching the process environment.
fn private_url_from(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    get("TEST_REDIS_PRIVATE_URL")
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn image() -> String {
    match std::env::var("TEST_REDIS_IMAGE") {
        Ok(image) if !image.trim().is_empty() => image.trim().to_owned(),
        _ => IMAGE.to_owned(),
    }
}

async fn start_container() -> String {
    let image = image();
    let name = format!("conveyor-redis-test-{}", Uuid::now_v7().simple());
    let output = Command::new("docker")
        .args([
            "run",
            "-d",
            "--rm",
            "--name",
            &name,
            // The server's RDB lands here, so nothing touches the host disk and `--rm` leaves
            // no volume behind.
            "--tmpfs",
            "/data",
            "-p",
            "127.0.0.1::6379",
            &image,
        ])
        .output()
        .expect(
            "no TEST_REDIS_URL/REDIS_URL and `docker` cannot run: integration tests fail closed",
        );
    assert!(
        output.status.success(),
        "docker run {image} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    spawn_watcher(&id);

    let port_output = Command::new("docker")
        .args(["port", &id, "6379/tcp"])
        .output()
        .expect("docker port");
    let mapping = String::from_utf8_lossy(&port_output.stdout).into_owned();
    let port = mapping
        .lines()
        .next()
        .and_then(|line| line.rsplit(':').next())
        .map(str::trim)
        .filter(|port| !port.is_empty())
        .unwrap_or_else(|| {
            remove(&name);
            panic!("no published port for {id}: {mapping:?}")
        })
        .to_owned();

    let url = format!("redis://127.0.0.1:{port}/");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        // `Bus::connect` completes the handshake (and therefore proves the server answers), so it
        // is the readiness probe as well as what the tests will use.
        match Bus::connect(&url).await {
            Ok(_) => return url,
            Err(error) if Instant::now() > deadline => {
                remove(&name);
                panic!("redis never became ready on port {port}: {error}");
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
}

fn remove(container: &str) {
    let _ = Command::new("docker")
        .args(["rm", "-f", "-v", container])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Remove the container when this test process dies — including a panic or a kill, where a `Drop`
/// in the test would not run. The timeout is the Postgres half's: six hours, then give up rather
/// than leak a `sh` per abandoned test process.
fn spawn_watcher(container: &str) {
    let pid = std::process::id();
    let script = format!(
        "i=0; while kill -0 {pid} 2>/dev/null && [ $i -lt 21600 ]; do sleep 1; i=$((i+1)); done; \
         docker rm -f -v {container} >/dev/null 2>&1"
    );
    let _ = Command::new("sh")
        .arg("-c")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::{private_url_from, private_url_with};

    #[test]
    fn a_private_url_from_the_environment_wins() {
        let named = |value: &'static str| {
            move |key: &str| (key == "TEST_REDIS_PRIVATE_URL").then(|| value.to_owned())
        };
        assert_eq!(
            private_url_from(named("redis://127.0.0.1:6380/")).as_deref(),
            Some("redis://127.0.0.1:6380/")
        );
        assert_eq!(
            private_url_from(|_| None),
            None,
            "unset falls back to docker"
        );
        assert_eq!(
            private_url_from(|_| Some("  ".to_owned())),
            None,
            "blank falls back to docker"
        );
    }

    /// The seam is what makes the branch load-bearing: if the `if let` in `private_url_with` is
    /// removed, this test receives the fallback and fails. The fallback PANICS rather than
    /// resolving: a start that happens above the branch must fail too, not pass because its future
    /// was ready.
    #[tokio::test]
    async fn a_private_url_from_the_environment_wins_without_starting_a_container() {
        let url = private_url_with(
            |key| (key == "TEST_REDIS_PRIVATE_URL").then(|| "redis://127.0.0.1:6380/".to_owned()),
            std::future::poll_fn(|_| -> std::task::Poll<String> {
                panic!("the environment URL must short-circuit before a container is started")
            }),
        )
        .await;
        assert_eq!(url, "redis://127.0.0.1:6380/");
    }

    #[tokio::test]
    async fn a_private_url_falls_back_to_the_container() {
        let url =
            private_url_with(|_| None, std::future::ready("docker-container".to_owned())).await;
        assert_eq!(url, "docker-container");
    }
}
