//! Knowledge design — the effective configuration of a released position and the department
//! conflicts around it (T18-5, #566; SPEC_V2 §10.5.2).
//!
//! The effective configuration is the position's immutable release definition
//! (`job_releases.list`), not the editable draft: a release is what a binding pins. The
//! department view joins the position's ordered placements (`job_catalogue.list` →
//! `departments.list`) and compares the anatomy of every other position in the same department;
//! a differing `model_ref`, `knowledge`, `memory`, `executor_kind` or `prompt_recipe` is a real
//! divergence in the tenant's rows, shown with both values. Nothing is invented: a position with
//! no release says "draft", and a department with no other position says "no comparison".

use std::collections::HashMap;

use leptos::prelude::*;
use serde_json::Value;

use crate::components::{Empty, ErrorPanel, Planned, StatusPill};
use crate::screens::v2_data::{read_rows, text};
use crate::state::AppCtx;

/// The anatomy keys the department comparison reads. They are the fields a knowledge pack, a
/// model or an executor choice would make diverge between two positions in one department.
const COMPARED_KEYS: [&str; 5] = [
    "model_ref",
    "knowledge",
    "memory",
    "executor_kind",
    "prompt_recipe",
];

#[component]
pub fn KnowledgeScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let refresh = RwSignal::new(0_u32);
    let selected_job = RwSignal::new(Option::<String>::None);

    let jobs = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            if !ctx.has_action("job_catalogue.list") {
                return Ok(Vec::new());
            }
            read_rows(&client, "job_catalogue.list", serde_json::json!({})).await
        }
    });

    Effect::new(move |_| {
        let Some(Ok(rows)) = jobs.get() else {
            return;
        };
        let current = selected_job.get_untracked();
        let still_there = current
            .as_deref()
            .is_some_and(|id| rows.iter().any(|row| text(row, "id") == id));
        if !still_there {
            selected_job.set(rows.first().map(|row| text(row, "id")));
        }
    });

    let departments = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            if !ctx.has_action("departments.list") {
                return Ok(Vec::new());
            }
            read_rows(&client, "departments.list", serde_json::json!({})).await
        }
    });

    let releases = LocalResource::new(move || {
        let job = selected_job.get();
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            let Some(job) = job else {
                return Ok(Vec::new());
            };
            if !ctx.has_action("job_releases.list") {
                return Ok(Vec::new());
            }
            read_rows(
                &client,
                "job_releases.list",
                serde_json::json!({ "job_id": job }),
            )
            .await
        }
    });

    let retry = Callback::new(move |_| refresh.update(|n| *n += 1));
    let department_names = move || -> HashMap<String, String> {
        match departments.get() {
            Some(Ok(rows)) => rows
                .into_iter()
                .map(|row| (text(&row, "id"), text(&row, "name")))
                .collect(),
            _ => HashMap::new(),
        }
    };

    view! {
        <section class="screen" data-testid="knowledge-design">
            <div class="screen-head">
                <h1>"Knowledge design"</h1>
                <p class="muted">
                    "The effective configuration of a released position — its knowledge layers, model,
                    memory, prompt and grants — and the department placements, compared against the
                    other positions that share them."
                </p>
            </div>

            {move || {
                if !ctx.has_action("job_catalogue.list") {
                    view! {
                        <Planned
                            title="Knowledge design".to_owned()
                            detail="job_catalogue.list is not in this role's manifest. The design view appears when the action lands, not before.".to_owned()
                        />
                    }.into_any()
                } else {
                    view! {
                        <label class="filter">
                            <span>"Position"</span>
                            <select on:change=move |event| {
                                let value = event_target_value(&event);
                                selected_job.set((!value.is_empty()).then_some(value));
                            }>
                                <option value="">"Choose a position"</option>
                                {move || match jobs.get() {
                                    Some(Ok(rows)) => rows.into_iter().map(|row| {
                                        let id = text(&row, "id");
                                        let id_for_value = id.clone();
                                        let id_for_selected = id.clone();
                                        view! {
                                            <option
                                                value=id_for_value
                                                selected=move || selected_job.get().as_deref() == Some(id_for_selected.as_str())
                                            >
                                                {format!("{} · {}", text(&row, "key"), text(&row, "title"))}
                                            </option>
                                        }
                                    }).collect_view().into_any(),
                                    _ => view! { <option value="">"Reading positions…"</option> }.into_any(),
                                }}
                            </select>
                        </label>

                        <div class="grid two">
                            <div class="panel" data-testid="knowledge-effective">
                                <h2>"The effective configuration"</h2>
                                {move || match releases.get() {
                                    None => view! { <p class="muted">"Reading releases…"</p> }.into_any(),
                                    Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                    Some(Ok(rows)) => {
                                        let job = selected_job.get();
                                        let job_row = match jobs.get() {
                                            Some(Ok(rows)) => rows.into_iter().find(|row| Some(text(row, "id")) == job),
                                            _ => None,
                                        };
                                        let latest = rows.into_iter().next();
                                        let (definition, label) = match latest {
                                            Some(release) => (
                                                release.get("definition").cloned().unwrap_or(Value::Null),
                                                format!("release {} · {}", text(&release, "release_hash"), text(&release, "created_at")),
                                            ),
                                            None => (
                                                job_row.as_ref().and_then(|row| row.get("definition").cloned()).unwrap_or(Value::Null),
                                                "draft — no release is published for this position yet".to_owned(),
                                            ),
                                        };
                                        if definition.is_null() {
                                            return view! {
                                                <Empty message="This position carries no anatomy yet. A release freezes it; the effective configuration appears here once it exists.".to_owned()/>
                                            }.into_any();
                                        }
                                        let rows_view = match definition.as_object() {
                                            Some(object) => object
                                                .iter()
                                                .map(|(key, value)| view! {
                                                    <tr>
                                                        <td class="mono">{key.clone()}</td>
                                                        <td>{value.to_string()}</td>
                                                    </tr>
                                                })
                                                .collect_view()
                                                .into_any(),
                                            None => view! { <tr><td colspan="2">"—"</td></tr> }.into_any(),
                                        };
                                        view! {
                                            <p class="muted" data-testid="knowledge-source">{label}</p>
                                            <table class="table">
                                                <thead><tr><th>"Key"</th><th>"Value"</th></tr></thead>
                                                <tbody>{rows_view}</tbody>
                                            </table>
                                        }.into_any()
                                    }
                                }}
                            </div>

                            <div class="panel" data-testid="knowledge-departments">
                                <h2>"Departments and conflicts"</h2>
                                {move || match jobs.get() {
                                    None => view! { <p class="muted">"Reading positions…"</p> }.into_any(),
                                    Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                    Some(Ok(rows)) => {
                                        let Some(selected) = selected_job.get() else {
                                            return view! { <Empty message="Choose a position to read its departments.".to_owned()/> }.into_any();
                                        };
                                        let Some(job) = rows.iter().find(|row| text(row, "id") == selected) else {
                                            return view! { <Empty message="That position is not in the tenant's catalogue.".to_owned()/> }.into_any();
                                        };
                                        let names = department_names();
                                        let placements: Vec<(String, String)> = job
                                            .get("departments")
                                            .and_then(Value::as_array)
                                            .map(|rows| rows.iter().map(|row| {
                                                let id = text(row, "department_id");
                                                let name = names.get(&id).cloned().unwrap_or_else(|| id.clone());
                                                (id, name)
                                            }).collect())
                                            .unwrap_or_default();
                                        let definition = job.get("definition").cloned().unwrap_or(Value::Null);
                                        let mut conflicts: Vec<(String, String, String, String)> = Vec::new();
                                        for (department_id, department_name) in &placements {
                                            for other in &rows {
                                                if text(other, "id") == selected {
                                                    continue;
                                                }
                                                let shares = other
                                                    .get("departments")
                                                    .and_then(Value::as_array)
                                                    .is_some_and(|rows| rows.iter().any(|row| text(row, "department_id") == *department_id));
                                                if !shares {
                                                    continue;
                                                }
                                                let other_definition = other.get("definition").cloned().unwrap_or(Value::Null);
                                                for key in COMPARED_KEYS {
                                                    let mine = definition.get(key).cloned().unwrap_or(Value::Null);
                                                    let theirs = other_definition.get(key).cloned().unwrap_or(Value::Null);
                                                    if mine != theirs && !(mine.is_null() && theirs.is_null()) {
                                                        conflicts.push((
                                                            department_name.clone(),
                                                            text(other, "title"),
                                                            key.to_owned(),
                                                            format!("{} vs {}", mine, theirs),
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                        view! {
                                            <ul>
                                                {placements.into_iter().map(|(_, name)| view! { <li><StatusPill status="placed".to_owned()/> " " {name}</li> }).collect_view()}
                                            </ul>
                                            {if conflicts.is_empty() {
                                                view! { <p class="muted">"No department conflict: every other position in these departments declares the same compared anatomy."</p> }.into_any()
                                            } else {
                                                view! {
                                                    <h3>"Conflicts"</h3>
                                                    <table class="table" data-testid="knowledge-conflicts">
                                                        <thead><tr><th>"Department"</th><th>"Position"</th><th>"Key"</th><th>"Values"</th></tr></thead>
                                                        <tbody>
                                                            {conflicts.into_iter().map(|(department, position, key, values)| view! {
                                                                <tr>
                                                                    <td>{department}</td>
                                                                    <td>{position}</td>
                                                                    <td class="mono">{key}</td>
                                                                    <td>{values}</td>
                                                                </tr>
                                                            }).collect_view()}
                                                        </tbody>
                                                    </table>
                                                }.into_any()
                                            }}
                                        }.into_any()
                                    }
                                }}
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </section>
    }
}
