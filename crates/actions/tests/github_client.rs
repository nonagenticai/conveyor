//! #70 P5: the GitHub transport's exact wire — the ref→commit→tree reads, one blob/tree/commit,
//! the branch ref and the pull request. A local stub records every body; no network leaves the box.
//!
//! `cargo test -p conveyor-actions --test github_client`.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::to_bytes;
use axum::extract::{Request, State};
use axum::response::{IntoResponse, Response};
use conveyor_actions::integrations::github::{GitHub, RepoRef, TreeChange};
use serde_json::{Value, json};

#[derive(Clone, Default)]
struct Stub {
    recorded: Arc<Mutex<Vec<(String, Value)>>>,
}

async fn handler(State(stub): State<Stub>, request: Request) -> Response {
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    let body = to_bytes(request.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let parsed: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };
    stub.recorded
        .lock()
        .expect("record")
        .push((format!("{method} {path}"), parsed));

    let value = match (method.as_str(), path.as_str()) {
        ("GET", "/repos/o/r/git/ref/heads/main") => json!({
            "object": {"sha": "base-commit", "type": "commit"}
        }),
        ("GET", "/repos/o/r/git/commits/base-commit") => json!({
            "tree": {"sha": "base-tree"}
        }),
        ("GET", "/repos/o/r/git/trees/base-tree") => json!({
            "truncated": false,
            "tree": [
                {"path": "design", "type": "tree", "sha": "dir-sha"},
                {"path": "design/keep.html", "type": "blob", "sha": "keep-blob"}
            ]
        }),
        ("POST", "/repos/o/r/git/blobs") => json!({"sha": "new-blob"}),
        ("POST", "/repos/o/r/git/trees") => json!({"sha": "new-tree"}),
        ("POST", "/repos/o/r/git/commits") => json!({"sha": "new-commit"}),
        ("POST", "/repos/o/r/git/refs") => json!({"ref": "refs/heads/publish/design-test"}),
        ("POST", "/repos/o/r/pulls") => json!({
            "number": 7,
            "html_url": "https://example.test/o/r/pull/7"
        }),
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                axum::Json(json!({"message": "no stub"})),
            )
                .into_response();
        }
    };
    (axum::http::StatusCode::CREATED, axum::Json(value)).into_response()
}

#[tokio::test]
async fn the_publish_transport_speaks_the_git_data_api() {
    let stub = Stub::default();
    let app = Router::new().fallback(handler).with_state(stub.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let github = GitHub::with_api("test-token", &format!("http://{addr}")).expect("client");
    let repo = RepoRef {
        owner: "o".to_owned(),
        name: "r".to_owned(),
    };

    assert_eq!(
        github.branch_head(&repo, "main").await.expect("ref"),
        "base-commit"
    );
    assert_eq!(
        github
            .commit_tree(&repo, "base-commit")
            .await
            .expect("commit"),
        "base-tree"
    );
    let tree = github.tree(&repo, "base-tree").await.expect("tree");
    assert_eq!(tree.len(), 1, "directory entries are not blobs");
    assert_eq!(tree[0].path, "design/keep.html");

    assert_eq!(
        github.create_blob(&repo, b"hello\n").await.expect("blob"),
        "new-blob"
    );
    let tree_sha = github
        .create_tree(
            &repo,
            "base-tree",
            &[TreeChange {
                path: "design/new.html".to_owned(),
                sha: Some("new-blob".to_owned()),
            }],
        )
        .await
        .expect("create tree");
    assert_eq!(tree_sha, "new-tree");
    let commit = github
        .create_commit(&repo, "message", &tree_sha, &["base-commit".to_owned()])
        .await
        .expect("create commit");
    assert_eq!(commit, "new-commit");
    github
        .create_ref(&repo, "publish/design-test", &commit)
        .await
        .expect("create ref");
    let pull = github
        .open_pull(&repo, "title", "body", "publish/design-test", "main")
        .await
        .expect("pull");
    assert_eq!(pull.number, 7);
    assert_eq!(pull.html_url, "https://example.test/o/r/pull/7");

    let recorded = stub.recorded.lock().expect("recorded").clone();
    let blob = recorded
        .iter()
        .find(|(path, _)| path == "POST /repos/o/r/git/blobs")
        .map(|(_, body)| body)
        .expect("a blob body");
    assert_eq!(blob["encoding"], "base64");
    assert_eq!(blob["content"], "aGVsbG8K", "the blob is the exact bytes");
    let tree_body = recorded
        .iter()
        .find(|(path, _)| path == "POST /repos/o/r/git/trees")
        .map(|(_, body)| body)
        .expect("a tree body");
    assert_eq!(tree_body["base_tree"], "base-tree");
    assert_eq!(tree_body["tree"][0]["path"], "design/new.html");
    assert_eq!(tree_body["tree"][0]["sha"], "new-blob");
}
