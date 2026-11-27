#![forbid(unsafe_code)]
//! The same-origin static UI (the approved one-image serving decision, 2026-11-09).
//!
//! The Leptos CSR bundle is baked into the image at `/srv/site`; `CONVEYOR_WEB_ROOT` overrides
//! that for a local run. With no root this build is API-only and every unmatched path keeps the
//! RFC 9457 404 — which is what the tests and a headless deployment see.
//!
//! `/config.json` is generated from the process configuration, never shipped as a file: a
//! browser must never read a stale `dev_auth: true` copied from the development assets.

use std::path::{Component, Path, PathBuf};

use axum::body::Body;
use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::config::normalize_host;
use crate::state::AppState;

/// `POST /__dev/login?login=<name>` — the local dev cookie, the same contract `dev-server.mjs`
/// used to proxy.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct DevLoginQuery {
    #[serde(default)]
    login: Option<String>,
}

/// Set the dev cookie and send the browser back to the app.
///
/// Registered on every build, but it only acts when dev authentication is actually enabled: in
/// production (or with `CONVEYOR_DEV_AUTH` unset) it is the same RFC 9457 404 as any unknown
/// route, so a deployed image cannot mint a dev session. The UI's `auth::dev_login` posts here.
pub(crate) async fn dev_login(
    State(state): State<AppState>,
    Query(query): Query<DevLoginQuery>,
) -> Response {
    let enabled =
        state.config.auth.dev_auth && conveyor_auth::is_development_env(&state.config.app_env);
    if !enabled {
        return crate::error::ApiError::not_found("no route for POST /__dev/login").into_response();
    }
    let login = query.login.unwrap_or_else(|| "alice".to_owned());
    let valid = !login.is_empty()
        && login.len() <= 128
        && login
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !valid {
        return (StatusCode::BAD_REQUEST, "invalid dev login").into_response();
    }
    // The fetch follows the redirect; the screen then reloads into an authenticated boot. The
    // cookie is deliberately not HttpOnly/Secure: it is the dev-only transport and the page's
    // own API calls must not need it — the server resolves it from the cookie either way.
    let cookie = format!("conveyor_dev={login}; Path=/; SameSite=Lax");
    let mut headers = HeaderMap::new();
    headers.insert(header::LOCATION, HeaderValue::from_static("/"));
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        headers.insert(header::SET_COOKIE, value);
    }
    (StatusCode::SEE_OTHER, headers).into_response()
}

/// What `/config.json` answers: the values the API's own OIDC validation used at startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteConfig {
    /// The realm issuer; empty when only dev authentication is configured.
    pub issuer: String,
    /// The public client the PKCE flow uses.
    pub client_id: String,
    /// True only in a development/test environment with `CONVEYOR_DEV_AUTH` set.
    pub dev_auth: bool,
}

/// The UI's `/config.json` view of the effective server configuration, for one host.
///
/// The game host answers its own public client; every other host keeps the operator client.
pub(crate) fn site_config(state: &AppState, host: &str) -> SiteConfig {
    let auth = &state.config.auth;
    let client_id = if state.config.is_game_host(host) {
        state
            .config
            .game_client_id()
            .unwrap_or(auth.oidc_client_id.as_str())
            .to_owned()
    } else {
        auth.oidc_client_id.clone()
    };
    SiteConfig {
        issuer: auth.oidc_issuer.clone().unwrap_or_default(),
        client_id,
        dev_auth: auth.dev_auth && conveyor_auth::is_development_env(&state.config.app_env),
    }
}

/// Which static tree a request is dispatched to, and the configured game origin.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HostDispatch<'a> {
    pub game: bool,
    pub game_public_url: Option<&'a str>,
}

impl HostDispatch<'_> {
    #[cfg(test)]
    fn operator() -> Self {
        Self {
            game: false,
            game_public_url: None,
        }
    }
}

/// The request's normalised `Host`. Empty when the header is absent or not UTF-8.
fn request_host(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(normalize_host)
        .unwrap_or_default()
}

/// The game host's hard boundary, applied **outside** the router.
///
/// Axum routes pre-empt the fallback, so a check inside `not_found_or_web` could never stop
/// `/mcp` (a real route) or the live design window. This middleware runs before both: on a
/// configured game host, the operator-only surfaces are a 404.
pub(crate) async fn game_host_gate(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if state.config.is_game_host(&request_host(request.headers()))
        && is_operator_surface(request.uri().path())
    {
        let method = request.method().clone();
        let uri = request.uri().clone();
        return crate::not_found(method, uri).await.into_response();
    }
    next.run(request).await
}

/// The surfaces that exist only on the operator origin: the MCP transport and the live
/// design window. Everything else is judged by `serve_host` (walk tree vs operator tree).
fn is_operator_surface(path: &str) -> bool {
    path == "/mcp" || path.starts_with("/mcp/") || path == "/design" || path.starts_with("/design/")
}

/// The router fallback: the live design window, then the static UI, then the API's 404.
pub(crate) async fn not_found_or_web(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let host = request_host(&headers);
    let game = state.config.is_game_host(&host);
    // The live window answers first on the operator origin: these paths must come from
    // Postgres, never from a stale file baked into the image. It has no business on the game
    // host (the middleware refuses it there too; this is the belt-and-braces).
    if !game && let Some(response) = crate::design::serve(&state, &method, &uri, &headers).await {
        return response;
    }
    let root = web_root();
    let dispatch = HostDispatch {
        game,
        game_public_url: state.config.game_public_url.as_deref(),
    };
    if let Some(response) = serve_host(
        &site_config(&state, &host),
        dispatch,
        root.as_deref(),
        &method,
        &uri,
    )
    .await
    {
        return response;
    }
    crate::not_found(method, uri).await.into_response()
}

/// Answer an unmatched request from the site root, or `None` when this is not a UI request.
///
/// `/api`, `/mcp`, the health probes and `/metrics` never reach the static path, so an unknown
/// API route stays a JSON problem instead of a 200 `index.html` — the failure a SPA fallback
/// normally hides.
/// The pre-host test entry point: the operator shape with no game redirect configured.
#[cfg(test)]
async fn serve(
    config: &SiteConfig,
    root: Option<&Path>,
    method: &Method,
    uri: &Uri,
) -> Option<Response> {
    serve_host(config, HostDispatch::operator(), root, method, uri).await
}

/// A permanent redirect to `location` (a path or an absolute URL), preserving the query.
fn redirect_to(location: &str, query: Option<&str>) -> Option<Response> {
    let location = match query {
        Some(query) => format!("{location}?{query}"),
        None => location.to_owned(),
    };
    let mut headers = HeaderMap::new();
    headers.insert(header::LOCATION, HeaderValue::from_str(&location).ok()?);
    Some((StatusCode::PERMANENT_REDIRECT, headers).into_response())
}

async fn serve_host(
    config: &SiteConfig,
    dispatch: HostDispatch<'_>,
    root: Option<&Path>,
    method: &Method,
    uri: &Uri,
) -> Option<Response> {
    if *method != Method::GET && *method != Method::HEAD {
        return None;
    }
    let path = uri.path();
    if is_api_path(path) {
        return None;
    }
    if path == "/config.json" {
        return Some(config_response(config));
    }

    // A legacy /walk path on the game host goes to the same host's root remainder. This needs
    // no web root, so it answers in an API-only build too.
    if dispatch.game && (path == "/walk" || path.starts_with("/walk/")) {
        let rest = &path["/walk".len()..];
        let rest = if rest.is_empty() { "/" } else { rest };
        return redirect_to(rest, uri.query());
    }

    // The operator origin: the old /walk prefix is a 308 to the configured game origin. It
    // is checked before any static lookup so `/walk` (no slash) does not 308 to `/walk/`
    // first. The target is configuration, never the request Host.
    if !dispatch.game
        && let Some(base) = dispatch.game_public_url
        && (path == "/walk" || path.starts_with("/walk/"))
    {
        let rest = &path["/walk".len()..];
        let rest = if rest.is_empty() { "/" } else { rest };
        return redirect_to(&format!("{base}{rest}"), uri.query());
    }

    let root = root?;

    // The game host is a separate application: the walk tree at the root, `/config.json`
    // and the API. It never serves the operator index, a directory index or the design pack.
    if dispatch.game {
        let relative = sanitize(path)?;
        if relative.extension().is_some() {
            let file = root.join("walk").join(&relative);
            if let Ok(bytes) = tokio::fs::read(&file).await {
                return Some(file_response(
                    file.extension().and_then(|ext| ext.to_str()).unwrap_or(""),
                    bytes,
                    method,
                ));
            }
            // A missing walk asset is a 404, never the SPA entry.
            return None;
        }
        let bytes = tokio::fs::read(root.join("walk/index.html")).await.ok()?;
        return Some(file_response("html", bytes, method));
    }

    let relative = sanitize(path)?;
    if relative.extension().is_some() {
        let file = root.join(&relative);
        if let Ok(bytes) = tokio::fs::read(&file).await {
            return Some(file_response(
                file.extension().and_then(|ext| ext.to_str()).unwrap_or(""),
                bytes,
                method,
            ));
        }
        // A missing asset is a 404, never the SPA entry: a silently served HTML file where the
        // browser expected JavaScript is the hardest kind of bundle failure to diagnose.
        return None;
    }
    // A directory that carries its own index is a static sub-site (the design pack at
    // /design). The slashless form redirects first: the page's assets are relative, so the
    // document has to be served from the directory URL, not from the parent path.
    let directory = root.join(&relative);
    if !relative.as_os_str().is_empty()
        && tokio::fs::metadata(directory.join("index.html"))
            .await
            .is_ok_and(|metadata| metadata.is_file())
    {
        if !path.ends_with('/') {
            let location = match uri.query() {
                Some(query) => format!("{path}/?{query}"),
                None => format!("{path}/"),
            };
            let mut headers = HeaderMap::new();
            headers.insert(header::LOCATION, HeaderValue::from_str(&location).ok()?);
            return Some((StatusCode::PERMANENT_REDIRECT, headers).into_response());
        }
        let bytes = tokio::fs::read(directory.join("index.html")).await.ok()?;
        return Some(file_response("html", bytes, method));
    }
    // The no-host dev/CI shape: the walk remains a scoped sub-site under /walk/. On a
    // deployment with a game host configured the 308 above has already answered.
    if path.starts_with("/walk/") {
        let bytes = tokio::fs::read(root.join("walk/index.html")).await.ok()?;
        return Some(file_response("html", bytes, method));
    }
    let bytes = tokio::fs::read(root.join("index.html")).await.ok()?;
    Some(file_response("html", bytes, method))
}

/// The site root: `CONVEYOR_WEB_ROOT` when set, else `/srv/site` when the image carries one.
pub(crate) fn web_root() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("CONVEYOR_WEB_ROOT") {
        let raw = raw.trim();
        if !raw.is_empty() {
            return Some(PathBuf::from(raw));
        }
    }
    let default = PathBuf::from("/srv/site");
    default.is_dir().then_some(default)
}

fn is_api_path(path: &str) -> bool {
    path == "/api"
        || path.starts_with("/api/")
        || path == "/mcp"
        || path.starts_with("/mcp/")
        || path == "/health"
        || path.starts_with("/health/")
        || path == "/metrics"
        || path == "/__dev"
        || path.starts_with("/__dev/")
}

/// A path that cannot escape the site root: only normal components are accepted.
fn sanitize(path: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in Path::new(path.trim_start_matches('/')).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

fn content_type(extension: &str) -> &'static str {
    match extension {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn file_response(extension: &str, bytes: Vec<u8>, method: &Method) -> Response {
    let length = bytes.len();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(extension)),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    // The bundle's file names are not content-hashed, so assets get a short cache and the
    // entry document none at all: a deploy is visible on the next reload.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if extension == "html" {
            "no-cache"
        } else {
            "public, max-age=300"
        }),
    );
    let body = if *method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(bytes)
    };
    (StatusCode::OK, headers, body).into_response()
}

fn config_response(config: &SiteConfig) -> Response {
    let body = json!({
        "issuer": config.issuer,
        "client_id": config.client_id,
        "dev_auth": config.dev_auth,
    });
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    (StatusCode::OK, headers, Body::from(body.to_string())).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SiteConfig {
        SiteConfig {
            issuer: "https://auth.example/realms/conveyor".to_owned(),
            client_id: "conveyor-web".to_owned(),
            dev_auth: false,
        }
    }

    async fn body_text(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the response body");
        String::from_utf8(bytes.to_vec()).expect("utf-8")
    }

    #[tokio::test]
    async fn the_entry_document_and_a_route_shaped_path_serve_index_html() {
        let root = std::env::temp_dir().join(format!("conveyor-web-unit-{}", std::process::id()));
        tokio::fs::create_dir_all(root.join("pkg"))
            .await
            .expect("the temp site");
        tokio::fs::write(root.join("index.html"), "<!doctype html><title>c</title>")
            .await
            .expect("index");
        tokio::fs::write(root.join("pkg/conveyor-web.js"), "console.log(1)")
            .await
            .expect("asset");

        for path in ["/", "/boards", "/runs/01a1"] {
            let response = serve(
                &config(),
                Some(&root),
                &Method::GET,
                &Uri::from_static(path),
            )
            .await
            .unwrap_or_else(|| panic!("{path} is a UI path"));
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert!(
                body_text(response).await.contains("<!doctype html>"),
                "{path}"
            );
        }

        let asset = serve(
            &config(),
            Some(&root),
            &Method::GET,
            &Uri::from_static("/pkg/conveyor-web.js"),
        )
        .await
        .expect("the asset");
        assert_eq!(
            asset.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );

        // A missing asset is not the SPA entry, and API-shaped paths never reach the site.
        assert!(
            serve(
                &config(),
                Some(&root),
                &Method::GET,
                &Uri::from_static("/pkg/missing.js")
            )
            .await
            .is_none()
        );
        assert!(
            serve(
                &config(),
                Some(&root),
                &Method::GET,
                &Uri::from_static("/api/nope")
            )
            .await
            .is_none()
        );
        // Without a root the build is API-only.
        assert!(
            serve(&config(), None, &Method::GET, &Uri::from_static("/"))
                .await
                .is_none()
        );

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn walk_deep_links_serve_the_walk_index() {
        let root =
            std::env::temp_dir().join(format!("conveyor-web-unit-walk-{}", std::process::id()));
        tokio::fs::create_dir_all(root.join("walk/src"))
            .await
            .expect("the walk site");
        tokio::fs::write(root.join("index.html"), "<!doctype html><title>app</title>")
            .await
            .expect("the app index");
        tokio::fs::write(
            root.join("walk/index.html"),
            "<!doctype html><title>walk</title>",
        )
        .await
        .expect("the walk index");
        tokio::fs::write(root.join("walk/src/main.js"), "console.log('walk')")
            .await
            .expect("the walk asset");

        for path in [
            "/walk/",
            "/walk/portfolio",
            "/walk/portfolio/01a1",
            "/walk/board/01a1",
        ] {
            let response = serve(
                &config(),
                Some(&root),
                &Method::GET,
                &Uri::from_static(path),
            )
            .await
            .unwrap_or_else(|| panic!("{path} is a walk path"));
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert!(
                body_text(response).await.contains("<title>walk</title>"),
                "{path} must serve the walk entry, not the operator app"
            );
        }

        let slashless = serve(
            &config(),
            Some(&root),
            &Method::GET,
            &Uri::from_static("/walk"),
        )
        .await
        .expect("the slashless walk path");
        assert_eq!(slashless.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(slashless.headers().get(header::LOCATION).unwrap(), "/walk/");

        let asset = serve(
            &config(),
            Some(&root),
            &Method::GET,
            &Uri::from_static("/walk/src/main.js"),
        )
        .await
        .expect("the walk asset");
        assert_eq!(
            asset.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );

        assert!(
            serve(
                &config(),
                Some(&root),
                &Method::GET,
                &Uri::from_static("/walk/src/missing.js"),
            )
            .await
            .is_none(),
            "a missing walk asset is a 404, never the fallback"
        );
        assert!(
            serve(
                &config(),
                Some(&root),
                &Method::GET,
                &Uri::from_static("/api/actions"),
            )
            .await
            .is_none(),
            "/api never reaches the walk fallback"
        );

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn a_directory_with_an_index_is_served_under_its_trailing_slash() {
        let root =
            std::env::temp_dir().join(format!("conveyor-web-unit-design-{}", std::process::id()));
        tokio::fs::create_dir_all(root.join("design/assets"))
            .await
            .expect("the temp site");
        tokio::fs::create_dir_all(root.join("design/architecture/diagrams"))
            .await
            .expect("the design diagram directory");
        tokio::fs::create_dir_all(root.join("pkg"))
            .await
            .expect("a directory without an index");
        tokio::fs::write(root.join("index.html"), "<!doctype html><title>app</title>")
            .await
            .expect("index");
        tokio::fs::write(
            root.join("design/index.html"),
            "<!doctype html><title>studio</title>",
        )
        .await
        .expect("the design index");
        tokio::fs::write(
            root.join("design/studio.html"),
            "<!doctype html><title>studio</title>",
        )
        .await
        .expect("the design entry");
        tokio::fs::write(root.join("design/assets/plex.css"), "body{}")
            .await
            .expect("the design asset");
        tokio::fs::write(
            root.join("design/architecture/diagrams/run-sequence.light.svg"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
        )
        .await
        .expect("the design diagram");
        tokio::fs::write(root.join("pkg/conveyor-web.js"), "console.log(1)")
            .await
            .expect("a site asset");

        // The slashless form redirects, preserving the query: the sub-site's assets are
        // relative, so the document must be served from /design/.
        let response = serve(
            &config(),
            Some(&root),
            &Method::GET,
            &Uri::from_static("/design"),
        )
        .await
        .expect("the slashless design path");
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/design/"
        );

        let response = serve(
            &config(),
            Some(&root),
            &Method::GET,
            &Uri::from_static("/design?tab=portfolio"),
        )
        .await
        .expect("the slashless design path with a query");
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/design/?tab=portfolio"
        );

        let response = serve(
            &config(),
            Some(&root),
            &Method::GET,
            &Uri::from_static("/design/"),
        )
        .await
        .expect("the design index");
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_text(response).await.contains("<title>studio</title>"));

        let response = serve(
            &config(),
            Some(&root),
            &Method::GET,
            &Uri::from_static("/design/studio.html"),
        )
        .await
        .expect("the design entry");
        assert_eq!(response.status(), StatusCode::OK);

        let response = serve(
            &config(),
            Some(&root),
            &Method::GET,
            &Uri::from_static("/design/assets/plex.css"),
        )
        .await
        .expect("the nested design asset");
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/css; charset=utf-8"
        );

        let response = serve(
            &config(),
            Some(&root),
            &Method::GET,
            &Uri::from_static("/design/architecture/diagrams/run-sequence.light.svg"),
        )
        .await
        .expect("the nested design diagram");
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/svg+xml"
        );

        // A missing file inside the sub-site is a 404, never the SPA entry.
        assert!(
            serve(
                &config(),
                Some(&root),
                &Method::GET,
                &Uri::from_static("/design/missing.css")
            )
            .await
            .is_none()
        );

        // A directory without its own index does not become a sub-site: the SPA entry stands.
        for path in ["/pkg", "/boards"] {
            let response = serve(
                &config(),
                Some(&root),
                &Method::GET,
                &Uri::from_static(path),
            )
            .await
            .unwrap_or_else(|| panic!("{path} is a UI path"));
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert!(
                body_text(response).await.contains("<title>app</title>"),
                "{path}"
            );
        }

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn config_json_is_generated_with_dev_auth_off_in_production() {
        let response = serve(
            &config(),
            None,
            &Method::GET,
            &Uri::from_static("/config.json"),
        )
        .await
        .expect("the config");
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("\"dev_auth\":false"), "{body}");
        assert!(body.contains("conveyor-web"), "{body}");
    }

    fn game_config() -> SiteConfig {
        SiteConfig {
            issuer: "https://auth.example/realms/conveyor".to_owned(),
            client_id: "conveyor-game".to_owned(),
            dev_auth: false,
        }
    }

    fn game_dispatch() -> HostDispatch<'static> {
        HostDispatch {
            game: true,
            game_public_url: None,
        }
    }

    #[tokio::test]
    async fn the_game_host_serves_the_walk_tree_at_the_root() {
        let root =
            std::env::temp_dir().join(format!("conveyor-web-unit-game-{}", std::process::id()));
        tokio::fs::create_dir_all(root.join("walk/src"))
            .await
            .expect("the walk tree");
        tokio::fs::write(
            root.join("index.html"),
            "<!doctype html><title>operator</title>",
        )
        .await
        .expect("the operator index");
        tokio::fs::write(
            root.join("walk/index.html"),
            "<!doctype html><title>walk</title>",
        )
        .await
        .expect("the walk index");
        tokio::fs::write(root.join("walk/src/main.js"), "console.log('walk')")
            .await
            .expect("the walk asset");

        // Every non-asset path is the walk entry, never the operator index.
        for path in ["/", "/portfolio", "/board/01a1"] {
            let response = serve_host(
                &game_config(),
                game_dispatch(),
                Some(&root),
                &Method::GET,
                &Uri::from_static(path),
            )
            .await
            .unwrap_or_else(|| panic!("{path} is a walk path"));
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            let body = body_text(response).await;
            assert!(body.contains("<title>walk</title>"), "{path}: {body}");
        }

        let asset = serve_host(
            &game_config(),
            game_dispatch(),
            Some(&root),
            &Method::GET,
            &Uri::from_static("/src/main.js"),
        )
        .await
        .expect("the walk asset");
        assert_eq!(
            asset.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );

        assert!(
            serve_host(
                &game_config(),
                game_dispatch(),
                Some(&root),
                &Method::GET,
                &Uri::from_static("/src/missing.js"),
            )
            .await
            .is_none(),
            "a missing walk asset is a 404, never the entry"
        );

        // `/config.json` answers the game client, without needing a web root.
        let response = serve_host(
            &game_config(),
            game_dispatch(),
            None,
            &Method::GET,
            &Uri::from_static("/config.json"),
        )
        .await
        .expect("the config");
        let body = body_text(response).await;
        assert!(body.contains("conveyor-game"), "{body}");

        // A legacy /walk path on the game host goes to the same host's root remainder.
        let legacy = serve_host(
            &game_config(),
            game_dispatch(),
            None,
            &Method::GET,
            &Uri::from_static("/walk/portfolio?x=1"),
        )
        .await
        .expect("the legacy redirect");
        assert_eq!(legacy.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            legacy.headers().get(header::LOCATION).unwrap(),
            "/portfolio?x=1"
        );

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn the_operator_host_redirects_walk_to_the_configured_game_origin() {
        let dispatch = HostDispatch {
            game: false,
            game_public_url: Some("https://game.nonagentic.ai"),
        };
        for (path, want) in [
            ("/walk", "https://game.nonagentic.ai/"),
            ("/walk/", "https://game.nonagentic.ai/"),
            ("/walk/portfolio", "https://game.nonagentic.ai/portfolio"),
            (
                "/walk/portfolio?x=1",
                "https://game.nonagentic.ai/portfolio?x=1",
            ),
            (
                "/walk/src/main.js",
                "https://game.nonagentic.ai/src/main.js",
            ),
        ] {
            let response = serve_host(
                &config(),
                dispatch,
                None,
                &Method::GET,
                &Uri::from_static(path),
            )
            .await
            .unwrap_or_else(|| panic!("{path} is a redirect"));
            assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT, "{path}");
            assert_eq!(
                response.headers().get(header::LOCATION).unwrap(),
                want,
                "{path}"
            );
        }
    }

    #[test]
    fn the_game_host_gate_refuses_only_operator_surfaces() {
        assert!(is_operator_surface("/mcp"));
        assert!(is_operator_surface("/mcp/"));
        assert!(is_operator_surface("/design"));
        assert!(is_operator_surface("/design/data/snapshot.js"));
        assert!(!is_operator_surface("/"));
        assert!(!is_operator_surface("/portfolio"));
        assert!(!is_operator_surface("/api/actions"));
        assert!(!is_operator_surface("/config.json"));
        assert!(!is_operator_surface("/health/ready"));
    }

    #[test]
    fn a_traversal_path_cannot_escape_the_root() {
        assert!(sanitize("/../../etc/passwd").is_none());
        assert!(sanitize("/a/../../b").is_none());
        assert_eq!(sanitize("/pkg/app.js"), Some(PathBuf::from("pkg/app.js")));
        assert_eq!(sanitize("/"), Some(PathBuf::new()));
    }
}
