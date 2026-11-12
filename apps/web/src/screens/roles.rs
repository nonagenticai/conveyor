//! Roles & permissions — the tenant's policy over the compiled role→scope table.
//!
//! The screen shows BOTH numbers: what the compiled default is, and what this tenant decided.
//! Only `member` and `viewer` are editable:
//!
//! - **Admin is fixed.** An admin who could narrow the admin role could lock the tenant out of
//!   the screen that restores it; the server refuses the write too (`roles.policy_set`).
//! - **The kernel is fixed.** `me`, `tenants`, `actions`, `events` and `members` cannot be
//!   revoked — otherwise a tenant could revoke its way out of recovery.
//!
//! A cell that equals its default stores no row: saving sends only the differences, and Reset
//! deletes them all. Advanced shows the raw scope spelling (`projects:write`); the default view
//! shows the resource and the verb in words.

use std::collections::BTreeMap;

use leptos::prelude::*;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{ErrorPanel, Planned};
use crate::models::{PolicyCell, PolicyView};
use crate::state::AppCtx;

/// The scopes no tenant may revoke, by resource.
const KERNEL: [&str; 5] = ["me", "tenants", "actions", "events", "members"];

/// The roles a tenant may configure.
const EDITABLE: [&str; 2] = ["member", "viewer"];

fn role_label(role: &str) -> &'static str {
    match role {
        "admin" => "Tenant admin",
        "member" => "Member",
        "viewer" => "Viewer",
        "kata_executor" => "Kata executor (machine)",
        _ => "Role",
    }
}

fn human_scope(scope: &str) -> String {
    let (resource, verb) = scope.split_once(':').unwrap_or((scope, ""));
    format!("{} · {}", resource.replace('_', " "), verb)
}

fn is_kernel(scope: &str) -> bool {
    scope
        .split_once(':')
        .map(|(resource, _)| KERNEL.contains(&resource))
        .unwrap_or(false)
}

fn is_editable(role: &str, scope: &str) -> bool {
    EDITABLE.contains(&role) && !is_kernel(scope)
}

fn load(ctx: AppCtx) -> LocalResource<Result<PolicyView, ApiError>> {
    LocalResource::new(move || async move {
        let _tenant = ctx.tenant.get();
        let output = ctx
            .client()
            .invoke(
                "roles.policy_get",
                serde_json::json!({}),
                InvokeOptions::default(),
            )
            .await?
            .output;
        serde_json::from_value(output)
            .map_err(|error| ApiError::Network(format!("role policy: {error}")))
    })
}

#[component]
pub fn Roles() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = load(ctx);
    let retry = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);
    // Pending toggles, keyed (role, scope). Absent = the loaded effective value.
    let draft = RwSignal::new(BTreeMap::<(String, String), bool>::new());

    let save = move |_| {
        let Some(Ok(policy)) = data.get() else {
            return;
        };
        let pending = draft.get_untracked();
        let entries: Vec<serde_json::Value> = policy
            .cells
            .iter()
            .filter_map(|cell| {
                let effective = pending
                    .get(&(cell.role.clone(), cell.scope.clone()))
                    .copied()
                    .unwrap_or(cell.allowed);
                // Only a DIFFERENCE from the compiled default is an override.
                (effective != cell.default).then(|| {
                    serde_json::json!({
                        "role": cell.role,
                        "scope": cell.scope,
                        "allowed": effective,
                    })
                })
            })
            .collect();
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "roles.policy_set",
                    serde_json::json!({ "entries": entries }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    draft.set(BTreeMap::new());
                    notice.set(Some("Policy saved. It applies on the next request.".into()));
                    data.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    let reset = move |_| {
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "roles.policy_reset",
                    serde_json::json!({}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    draft.set(BTreeMap::new());
                    notice.set(Some(
                        "Every override removed; the defaults apply again.".into(),
                    ));
                    data.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("roles.policy_get") {
                    return view! {
                        <Planned
                            title="Roles & permissions is planned".to_owned()
                            detail="roles.policy_get is not in this role's manifest. Only a tenant admin configures the policy.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Roles & permissions"</h1>
                            <p class="muted">
                                "What Members and Viewers may do. Admin is fixed, and the kernel scopes cannot be revoked."
                            </p>
                        </div>
                        <div class="filterbar">
                            <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                            <button class="btn primary" on:click=save>"Save changes"</button>
                            <button class="btn" on:click=reset>"Reset to defaults"</button>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <div class="panel notice" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Loading the policy…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(policy)) => view! {
                            <PolicyBody policy=policy draft=draft advanced=ctx.advanced/>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn PolicyBody(
    policy: PolicyView,
    draft: RwSignal<BTreeMap<(String, String), bool>>,
    advanced: RwSignal<bool>,
) -> impl IntoView {
    let roles = policy.roles.clone();
    let rows: Vec<(String, Vec<PolicyCell>)> = policy
        .scopes
        .iter()
        .map(|scope| {
            let cells = policy
                .cells
                .iter()
                .filter(|cell| &cell.scope == scope)
                .cloned()
                .collect::<Vec<_>>();
            (scope.clone(), cells)
        })
        .collect();
    let changed = move || draft.get().len();

    view! {
        <div class="panel">
            <p class="muted">
                "A dot marks a cell this tenant changed. Saving writes only the cells that differ from the default."
            </p>
            <p class="muted">{move || format!("{} pending change(s)", changed())}</p>
            <table class="table matrix">
                <thead>
                    <tr>
                        <th>"Permission"</th>
                        {roles.iter().map(|role| view! {
                            <th title=role.clone()>{role_label(role)}</th>
                        }).collect_view()}
                    </tr>
                </thead>
                <tbody>
                    {rows.into_iter().map(|(scope, cells)| {
                        let scope_for_label = scope.clone();
                        let kernel = is_kernel(&scope);
                        view! {
                            <tr>
                                <td>
                                    {move || if advanced.get() {
                                        view! { <span class="mono">{scope_for_label.clone()}</span> }.into_any()
                                    } else {
                                        view! { <span>{human_scope(&scope_for_label)}</span> }.into_any()
                                    }}
                                    {kernel.then(|| view! { <span class="pill muted">"locked"</span> })}
                                </td>
                                {cells.into_iter().map(|cell| {
                                    let key = (cell.role.clone(), cell.scope.clone());
                                    let editable = is_editable(&cell.role, &cell.scope);
                                    let default = cell.default;
                                    view! {
                                        <td>
                                            {move || {
                                                let effective = draft
                                                    .get()
                                                    .get(&key)
                                                    .copied()
                                                    .unwrap_or(cell.allowed);
                                                let overridden =
                                                    effective != default || cell.overridden;
                                                if editable {
                                                    let key = key.clone();
                                                    view! {
                                                        <label class="matrix-cell">
                                                            <input
                                                                type="checkbox"
                                                                prop:checked=effective
                                                                on:change=move |_| {
                                                                    let mut next = draft.get_untracked();
                                                                    let current = next.get(&key).copied().unwrap_or(cell.allowed);
                                                                    next.insert(key.clone(), !current);
                                                                    draft.set(next);
                                                                }
                                                            />
                                                            {overridden.then(|| view! { <span class="dot" title="changed from the default">"•"</span> })}
                                                        </label>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <span class="fixed" title="fixed">
                                                            {if effective { "✓" } else { "—" }}
                                                        </span>
                                                    }.into_any()
                                                }
                                            }}
                                        </td>
                                    }
                                }).collect_view()}
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}
