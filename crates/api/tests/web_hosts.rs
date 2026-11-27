//! The two-host serving contract (CR-WALK-001 / ADR 0024).
//!
//! The game host is a hard boundary: the router-level gate refuses the operator surfaces
//! (`/mcp`, `/design`) and `/config.json` answers the game client; the operator host is
//! unchanged and its legacy `/walk/*` prefix 308s to the configured game origin. These go
//! through `app()` so the axum route/fallback ordering is the real one, not a unit stub.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use support::{Pg, Running};

fn game_config(pg: &Pg) -> conveyor_api::config::Config {
    let mut config = support::config(pg);
    // The production operator client, so the per-host assertion is meaningful.
    config.auth.oidc_client_id = "conveyor-web".to_owned();
    config.game_hosts = vec!["game.nonagentic.ai".to_owned()];
    config.auth.oidc_game_client_id = Some("conveyor-game".to_owned());
    config.game_public_url = Some("https://game.nonagentic.ai".to_owned());
    config
        .validate_auth()
        .expect("the game host and its client id are a valid pair");
    config
}

#[tokio::test]
async fn the_game_host_refuses_operator_surfaces_and_answers_its_own_client() {
    let pg = Pg::fresh().await;
    let running = Running::start_with_config(game_config(&pg)).await;
    let game = [("host", "game.nonagentic.ai")];
    let operator = [("host", "conveyor.nonagentic.ai")];

    // /mcp is a real axum route, so only the outer middleware can refuse it.
    let (status, _) = running
        .http(reqwest::Method::POST, "/mcp", None, &game, None)
        .await;
    assert_eq!(status, 404, "the game host must not answer /mcp");

    // The game host's /config.json is its own public client.
    let (status, body) = running
        .http(reqwest::Method::GET, "/config.json", None, &game, None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["client_id"], "conveyor-game", "{body}");

    // The operator host keeps the operator client.
    let (status, body) = running
        .http(reqwest::Method::GET, "/config.json", None, &operator, None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["client_id"], "conveyor-web", "{body}");

    // The operator host keeps its MCP surface: not the gate's 404.
    let (status, _) = running
        .http(reqwest::Method::POST, "/mcp", None, &operator, None)
        .await;
    assert_ne!(status, 404, "the operator host still routes /mcp");

    // The legacy prefix is a 308 to configuration, with the query preserved. A client that
    // follows redirects would land on the live game host and hide the response under test.
    let (status, location) =
        redirect(&running, "/walk/portfolio?x=1", "conveyor.nonagentic.ai").await;
    assert_eq!(status, 308);
    assert_eq!(
        location.as_deref(),
        Some("https://game.nonagentic.ai/portfolio?x=1")
    );

    // A hostile Host cannot move the redirect: it is configuration, not the request.
    let (status, location) = redirect(&running, "/walk/portfolio", "attacker.example").await;
    assert_eq!(status, 308);
    assert_eq!(
        location.as_deref(),
        Some("https://game.nonagentic.ai/portfolio")
    );
}

/// The status and `Location` of a request whose redirects are NOT followed.
async fn redirect(running: &Running, path: &str, host: &str) -> (u16, Option<String>) {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("a no-redirect client");
    let response = client
        .get(format!("{}{}", running.base(), path))
        .header("host", host)
        .send()
        .await
        .expect("the request");
    (
        response.status().as_u16(),
        response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
    )
}
