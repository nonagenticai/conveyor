//! Validation — the measured programme: every project's status and its recorded evidence, read
//! from the tenant's own rows. The route is where a programme owner checks the record, not where
//! a fixture is rendered.

use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::ApiError;
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, fmt_int};
use crate::screens::v2_data::{read_rows, rows, text};
use crate::state::AppCtx;

#[derive(Clone, Debug, Default)]
struct ValidationData {
    projects: Vec<Value>,
    programmes: Vec<Value>,
}

async fn load(ctx: AppCtx) -> Result<ValidationData, ApiError> {
    let client = ctx.client();
    let value = client.get("/api/projects?limit=200").await?;
    let programmes = if ctx.has_action("programmes.list") {
        read_rows(
            &client,
            "programmes.list",
            json!({ "page": { "limit": 50 } }),
        )
        .await?
    } else {
        Vec::new()
    };
    Ok(ValidationData {
        projects: rows(&value),
        programmes,
    })
}

fn meta_text(row: &Value, key: &str) -> String {
    row.get("meta")
        .and_then(|meta| meta.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| "—".to_owned())
}

#[component]
pub fn ValidationScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen" aria-labelledby="validation-title">
            <header class="screen-head">
                <div>
                    <h1 id="validation-title">"Validation"</h1>
                    <p class="muted">"Every project's status and recorded evidence, from the tenant's own rows."</p>
                </div>
                <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            </header>
            {move || match data.get() {
                None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(data)) => view! { <ValidationBody data=data/> }.into_any(),
            }}
        </section>
    }
}

#[component]
fn ValidationBody(data: ValidationData) -> impl IntoView {
    let projects = data.projects;
    let programmes = data.programmes;
    let active = projects
        .iter()
        .filter(|row| text(row, "status") == "active")
        .count();
    let closed = projects
        .iter()
        .filter(|row| text(row, "status") == "closed")
        .count();
    let programme_name = {
        let programmes = programmes.clone();
        move |id: &str| -> String {
            programmes
                .iter()
                .find(|row| text(row, "id") == id)
                .map(|row| text(row, "name"))
                .unwrap_or_else(|| "—".to_owned())
        }
    };

    view! {
        <div class="grid-3">
            <Tile label="Projects".to_owned() value=fmt_int(projects.len() as i64) detail="tenant rows".to_owned()/>
            <Tile label="Active".to_owned() value=fmt_int(active as i64) detail="in flight".to_owned()/>
            <Tile label="Closed".to_owned() value=fmt_int(closed as i64) detail="recorded closure".to_owned()/>
        </div>
        <div class="grid-3">
            <Tile label="Programmes".to_owned() value=fmt_int(programmes.len() as i64) detail="tenant rows".to_owned()/>
        </div>
        {if projects.is_empty() {
            view! { <Empty message="No project in this tenant yet.".to_owned()/> }.into_any()
        } else {
            let programme_name = programme_name.clone();
            view! {
                <table class="table" data-testid="validation-table">
                    <thead><tr><th>"Key"</th><th>"Project"</th><th>"Status"</th><th>"Phase"</th><th>"Evidence"</th><th>"Programme"</th></tr></thead>
                    <tbody>
                        {projects.into_iter().map(|row| {
                            let programme_id = text(&row, "programme_id");
                            let programme = if programme_id == "—" { "—".to_owned() } else { programme_name(&programme_id) };
                            view! {
                                <tr>
                                    <td class="mono">{text(&row, "key")}</td>
                                    <td>{text(&row, "name")}</td>
                                    <td><StatusPill status=text(&row, "status")/></td>
                                    <td>{meta_text(&row, "phase")}</td>
                                    <td class="mono">{meta_text(&row, "evidence")}</td>
                                    <td>{programme}</td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}
    }
}
