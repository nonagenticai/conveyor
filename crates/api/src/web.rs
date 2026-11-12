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
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

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

/// The UI's `/config.json` view of the effective server configuration.
pub(crate) fn site_config(state: &AppState) -> SiteConfig {
    let auth = &state.config.auth;
    SiteConfig {
        issuer: auth.oidc_issuer.clone().unwrap_or_default(),
        client_id: auth.oidc_client_id.clone(),
        dev_auth: auth.dev_auth && conveyor_auth::is_development_env(&state.config.app_env),
    }
}

/// The router fallback: the static UI when this build carries one, otherwise the API's 404.
pub(crate) async fn not_found_or_web(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
) -> Response {
    let root = web_root();
    if let Some(response) = serve(&site_config(&state), root.as_deref(), &method, &uri).await {
        return response;
    }
    crate::not_found(method, uri).await.into_response()
}

/// Answer an unmatched request from the site root, or `None` when this is not a UI request.
///
/// `/api`, `/mcp`, the health probes and `/metrics` never reach the static path, so an unknown
/// API route stays a JSON problem instead of a 200 `index.html` — the failure a SPA fallback
/// normally hides.
async fn serve(
    config: &SiteConfig,
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
    let root = root?;
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
    let bytes = tokio::fs::read(root.join("index.html")).await.ok()?;
    Some(file_response("html", bytes, method))
}

/// The site root: `CONVEYOR_WEB_ROOT` when set, else `/srv/site` when the image carries one.
fn web_root() -> Option<PathBuf> {
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

    #[test]
    fn a_traversal_path_cannot_escape_the_root() {
        assert!(sanitize("/../../etc/passwd").is_none());
        assert!(sanitize("/a/../../b").is_none());
        assert_eq!(sanitize("/pkg/app.js"), Some(PathBuf::from("pkg/app.js")));
        assert_eq!(sanitize("/"), Some(PathBuf::new()));
    }
}
