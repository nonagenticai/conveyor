//! Identity and the tenant switch.
//!
//! Two modes, one bootstrap: `/api/me` decides whether the request is authenticated. Local
//! browser auth is the server's `conveyor_dev` cookie; production is auth-code + PKCE S256
//! with the access token held in memory (ADR 0001). `X-Active-Tenant` is sent from the
//! switcher; `/api/me` answers `200` with the membership list (`active_tenant: null`) before
//! one is chosen, which is what draws the switcher.

use base64::Engine;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestCredentials, RequestInit, RequestMode, Response, UrlSearchParams};

use crate::api::{ApiError, Client};
use crate::models::Me;

const VERIFIER_KEY: &str = "conveyor.pkce.verifier";
const STATE_KEY: &str = "conveyor.pkce.state";
const REDIRECT_PATH: &str = "/";

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub issuer: String,
    pub client_id: String,
    /// Set only by the local `public/config.json`; a deployed config leaves it false.
    #[serde(default)]
    pub dev_auth: bool,
}

/// Load `config.json` from the same origin. An absent file is not an error: the app then has
/// no OIDC path and can only use the dev cookie.
pub async fn load_config() -> Option<Config> {
    let value = Client::default().get("/config.json").await.ok()?;
    serde_json::from_value(value).ok()
}

pub async fn load_me(client: &Client) -> Result<Me, ApiError> {
    let value = client.get("/api/me").await?;
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("me: {error}")))
}

/// What a screen needs to decide whether a sign-in is possible.
#[derive(Clone, Debug, Default)]
pub struct AuthModes {
    pub dev: bool,
    pub oidc: bool,
    pub issuer: Option<String>,
}

impl AuthModes {
    pub fn from_config(config: &Option<Config>) -> Self {
        match config {
            Some(config) => Self {
                dev: config.dev_auth,
                oidc: !config.issuer.trim().is_empty() && !config.client_id.trim().is_empty(),
                issuer: Some(config.issuer.clone()),
            },
            None => Self {
                dev: true,
                oidc: false,
                issuer: None,
            },
        }
    }
}

/// The dev cookie is set by the same-origin server, then the document reloads into a boot
/// that succeeds. This is local/Playwright only; a deployed server refuses `CONVEYOR_DEV_AUTH`.
pub async fn dev_login(login: &str) -> Result<(), ApiError> {
    let window = web_sys::window().ok_or_else(|| ApiError::Network("no window".into()))?;
    let url = format!(
        "/__dev/login?login={}",
        js_sys::encode_uri_component(login)
            .as_string()
            .unwrap_or_default()
    );
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_mode(RequestMode::SameOrigin);
    init.set_credentials(RequestCredentials::SameOrigin);
    let request = Request::new_with_str_and_init(&url, &init).map_err(js)?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(js)?;
    let response: Response = response
        .dyn_into()
        .map_err(|_| ApiError::Network("dev login did not return a Response".into()))?;
    if !response.ok() {
        return Err(ApiError::Forbidden(
            "the server refused the dev login (is CONVEYOR_DEV_AUTH on?)".into(),
        ));
    }
    window.location().reload().map_err(js)?;
    Ok(())
}

/// Redirect to Keycloak with an S256 challenge. The verifier stays in `sessionStorage`
/// (never the token: the token is memory-only by design).
pub fn begin_login(config: &Config) -> Result<(), ApiError> {
    let window = web_sys::window().ok_or_else(|| ApiError::Network("no window".into()))?;
    let verifier = random_token(32)?;
    let state = random_token(16)?;
    let challenge = {
        let digest = Sha256::digest(verifier.as_bytes());
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
    };
    let storage = session_storage()?;
    storage.set_item(VERIFIER_KEY, &verifier).map_err(js)?;
    storage.set_item(STATE_KEY, &state).map_err(js)?;

    let redirect = format!(
        "{}{}",
        window.location().origin().map_err(js)?,
        REDIRECT_PATH
    );
    let url = format!(
        "{}/protocol/openid-connect/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        config.issuer.trim_end_matches('/'),
        encode(&config.client_id),
        encode(&redirect),
        encode("openid email profile"),
        encode(&challenge),
        encode(&state),
    );
    window.location().set_href(&url).map_err(js)?;
    Ok(())
}

/// Finish a Keycloak redirect: exchange the code, clear the URL, return the access token.
/// `Ok(None)` means this load is not a callback.
pub async fn complete_login(config: &Config) -> Result<Option<String>, ApiError> {
    let window = web_sys::window().ok_or_else(|| ApiError::Network("no window".into()))?;
    let search = window.location().search().map_err(js)?;
    if search.is_empty() {
        return Ok(None);
    }
    let params = UrlSearchParams::new_with_str(&search).map_err(js)?;
    let code = match params.get("code") {
        Some(code) if !code.is_empty() => code,
        _ => return Ok(None),
    };
    let returned_state = params.get("state").unwrap_or_default();
    let storage = session_storage()?;
    let expected_state = storage.get_item(STATE_KEY).map_err(js)?.unwrap_or_default();
    if expected_state.is_empty() || expected_state != returned_state {
        return Err(ApiError::Forbidden(
            "the sign-in response did not match this browser's request".into(),
        ));
    }
    let verifier = storage
        .get_item(VERIFIER_KEY)
        .map_err(js)?
        .ok_or_else(|| ApiError::Forbidden("the sign-in verifier is missing".into()))?;

    let redirect = format!(
        "{}{}",
        window.location().origin().map_err(js)?,
        REDIRECT_PATH
    );
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        encode(&code),
        encode(&redirect),
        encode(&config.client_id),
        encode(&verifier),
    );

    let init = RequestInit::new();
    init.set_method("POST");
    init.set_mode(RequestMode::Cors);
    init.set_credentials(RequestCredentials::Omit);
    let headers = web_sys::Headers::new().map_err(js)?;
    headers
        .set("content-type", "application/x-www-form-urlencoded")
        .map_err(js)?;
    init.set_headers(&headers);
    init.set_body(&JsValue::from_str(&body));
    let url = format!(
        "{}/protocol/openid-connect/token",
        config.issuer.trim_end_matches('/')
    );
    let request = Request::new_with_str_and_init(&url, &init).map_err(js)?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(js)?;
    let response: Response = response
        .dyn_into()
        .map_err(|_| ApiError::Network("the token endpoint did not return a Response".into()))?;
    let status = response.status();
    let text = JsFuture::from(response.text().map_err(js)?)
        .await
        .map_err(js)?
        .as_string()
        .unwrap_or_default();
    if !response.ok() {
        return Err(ApiError::Forbidden(format!(
            "the token endpoint refused the code ({status})"
        )));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| ApiError::Network(format!("token response: {error}")))?;
    let token = value
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::Network("the token response has no access_token".into()))?
        .to_owned();

    let _ = storage.remove_item(VERIFIER_KEY);
    let _ = storage.remove_item(STATE_KEY);
    let _ = window.history().and_then(|history| {
        history.replace_state_with_url(&JsValue::NULL, "", Some(REDIRECT_PATH))
    });
    Ok(Some(token))
}

fn random_token(bytes: usize) -> Result<String, ApiError> {
    let window = web_sys::window().ok_or_else(|| ApiError::Network("no window".into()))?;
    let crypto = window
        .crypto()
        .map_err(|_| ApiError::Network("no WebCrypto".into()))?;
    let mut buffer = vec![0u8; bytes];
    crypto
        .get_random_values_with_u8_array(&mut buffer)
        .map_err(|_| ApiError::Network("WebCrypto refused a random read".into()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer))
}

fn session_storage() -> Result<web_sys::Storage, ApiError> {
    web_sys::window()
        .ok_or_else(|| ApiError::Network("no window".into()))?
        .session_storage()
        .map_err(js)?
        .ok_or_else(|| ApiError::Network("sessionStorage is unavailable".into()))
}

fn encode(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_default()
}

fn js(value: JsValue) -> ApiError {
    ApiError::Network(value.as_string().unwrap_or_else(|| format!("{value:?}")))
}
