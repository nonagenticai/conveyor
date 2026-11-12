//! Members — the tenant's people, their roles, and the pending invitations.
//!
//! Three rules the screen keeps visible rather than hiding:
//!
//! - **The last admin cannot be demoted or removed.** The server refuses with `409 last_admin`;
//!   the screen shows the sentence and never removes the row it just failed to change.
//! - **A sole member is the tenant admin by default.** With one member the role control is
//!   disabled and the reason is written down, because "why can't I change this" is the question
//!   the screen must answer.
//! - **An invite is not a mail.** Nothing is sent; the invitee signs in with the invited address
//!   (verified) and the pending invite becomes their membership on that first sign-in.
//!
//! Every control is gated on the manifest: a role without `invites.*` sees the roster and no
//! invitation form, rather than a button that would 403.

use leptos::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, age};
use crate::models::{InviteView, MemberView};
use crate::state::AppCtx;

const ROLES: [(&str, &str); 3] = [
    ("admin", "Tenant admin"),
    ("member", "Member"),
    ("viewer", "Viewer"),
];

#[derive(Clone, Debug)]
struct MembersData {
    members: Vec<MemberView>,
    invites: Vec<InviteView>,
}

fn parse_page<T: serde::de::DeserializeOwned>(
    value: Value,
    what: &str,
) -> Result<Page<T>, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load(ctx: AppCtx) -> LocalResource<Result<MembersData, ApiError>> {
    LocalResource::new(move || async move {
        // Read every reactive input before the first await.
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        let members = parse_page::<MemberView>(
            client
                .invoke(
                    "members.list",
                    json!({"limit": 100}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
            "members",
        )?;
        // `invites.list` is members:write; a role that cannot manage members simply has no
        // invitations to show.
        let invites = if ctx.has_action("invites.list") {
            parse_page::<InviteView>(
                client
                    .invoke(
                        "invites.list",
                        json!({"limit": 50}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "invites",
            )?
            .items
        } else {
            Vec::new()
        };
        Ok(MembersData {
            members: members.items,
            invites,
        })
    })
}

#[component]
pub fn Members() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = load(ctx);
    let retry = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);
    let email = RwSignal::new(String::new());
    let invite_role = RwSignal::new("member".to_owned());
    let busy = RwSignal::new(false);

    let can_manage = move || ctx.has_action("members.set_role");

    let on_invite = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let address = email.get_untracked().trim().to_owned();
        if address.is_empty() {
            notice.set(Some("An email address is required.".into()));
            return;
        }
        let role = invite_role.get_untracked();
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "invites.create",
                    json!({"email": address, "role": role}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some(
                        "Invitation recorded. The invitee signs in with this address; nothing is emailed."
                            .into(),
                    ));
                    email.set(String::new());
                    data.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("members.list") {
                    return view! {
                        <Planned
                            title="Members is planned".to_owned()
                            detail="members.list is not in this role's manifest, so there is no roster to show.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Members"</h1>
                            <p class="muted">
                                "Who can act in this tenant, and as what. A tenant admin may be a single person."
                            </p>
                        </div>
                        <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <div class="panel notice" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Loading members…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=retry/>
                        }.into_any(),
                        Some(Ok(body)) => view! {
                            <MembersBody
                                data=body
                                notice=notice
                                can_manage=Signal::derive(can_manage)
                            />
                        }.into_any(),
                    }}
                    {move || (ctx.has_action("invites.create")).then(|| view! {
                        <form class="panel" on:submit=on_invite>
                            <h2>"Invite a person"</h2>
                            <p class="muted">
                                "Enter the address they sign in with. Nothing is emailed: the invitation binds on their first verified sign-in."
                            </p>
                            <div class="formrow">
                                <label class="filter">
                                    <span>"Email"</span>
                                    <input
                                        type="email"
                                        prop:value=move || email.get()
                                        on:input=move |event| email.set(event_target_value(&event))
                                        placeholder="name@example.com"
                                    />
                                </label>
                                <label class="filter">
                                    <span>"Role"</span>
                                    <select
                                        on:change=move |event| {
                                            if let Some(select) = event.target().and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                                                invite_role.set(select.value());
                                            }
                                        }
                                        aria-label="Invitation role"
                                    >
                                        {ROLES.iter().map(|(value, label)| view! {
                                            <option value=*value selected=move || invite_role.get() == *value>{*label}</option>
                                        }).collect_view()}
                                    </select>
                                </label>
                                <button class="btn primary" type="submit" disabled=move || busy.get()>
                                    {move || if busy.get() { "Inviting…" } else { "Create invitation" }}
                                </button>
                            </div>
                        </form>
                    })}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn MembersBody(
    data: MembersData,
    notice: RwSignal<Option<String>>,
    can_manage: Signal<bool>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let members = data.members.clone();
    let invites = data.invites.clone();
    let sole_member = members.len() == 1;
    let admin_count = members.iter().filter(|m| m.role == "admin").count();

    let on_role = move |user_id: String, version: i64, role: String| {
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "members.set_role",
                    json!({"user_id": user_id, "role": role}),
                    InvokeOptions {
                        if_match: Some(version),
                        idempotency_key: None,
                    },
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some("Role updated.".into()));
                    // The roster is a resource owned by the parent; reload through the page.
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().reload();
                    }
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    let on_remove = move |user_id: String| {
        let confirmed = web_sys::window()
            .and_then(|window| {
                window
                    .confirm_with_message("Remove this member from the tenant?")
                    .ok()
            })
            .unwrap_or(false);
        if !confirmed {
            return;
        }
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "members.remove",
                    json!({"user_id": user_id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some("Member removed.".into()));
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().reload();
                    }
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    let on_revoke = move |id: String| {
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "invites.revoke",
                    json!({"id": id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some("Invitation revoked.".into()));
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().reload();
                    }
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    let advanced = ctx.advanced;

    view! {
        <div class="panel">
            <h2>"Members"</h2>
            {if sole_member {
                view! {
                    <p class="muted">
                        "One member: this person is the tenant admin by default. Invite a second person before changing or removing this role."
                    </p>
                }.into_any()
            } else {
                view! { <p class="muted">{format!("{admin_count} tenant admin(s) of {} members", members.len())}</p> }.into_any()
            }}
            <table class="table">
                <thead>
                    <tr>
                        <th>"Person"</th>
                        <th>"Email"</th>
                        <th>"Role"</th>
                        <th>"Department"</th>
                        {move || advanced.get().then(|| view! { <th>"User id"</th> })}
                        <th>"Actions"</th>
                    </tr>
                </thead>
                <tbody>
                    {members.iter().map(|member| {
                        let user_id = member.user_id.clone();
                        let version = member.version;
                        let current_role = member.role.clone();
                        let remove_id = member.user_id.clone();
                        let display = if member.display_name.is_empty() {
                            member.email.clone().unwrap_or_else(|| member.user_id.clone())
                        } else {
                            member.display_name.clone()
                        };
                        view! {
                            <tr>
                                <td>{display}</td>
                                <td class="muted">{member.email.clone().unwrap_or_else(|| "—".into())}</td>
                                <td>
                                    {if sole_member || !can_manage.get() {
                                        view! { <span class="pill muted">{label_of(&current_role).to_owned()}</span> }.into_any()
                                    } else {
                                        let on_change = on_role;
                                        let user = user_id.clone();
                                        view! {
                                            <select
                                                aria-label="Member role"
                                                on:change=move |event| {
                                                    if let Some(select) = event.target().and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                                                        on_change(user.clone(), version, select.value());
                                                    }
                                                }
                                            >
                                                {ROLES.iter().map(|(value, label)| {
                                                    let selected = current_role == *value;
                                                    view! { <option value=*value selected=selected>{*label}</option> }
                                                }).collect_view()}
                                            </select>
                                        }.into_any()
                                    }}
                                </td>
                                <td class="muted">{member.department_id.clone().unwrap_or_else(|| "—".into())}</td>
                                {move || advanced.get().then(|| view! { <td class="mono">{short(&user_id)}</td> })}
                                <td>
                                    {if sole_member || !can_manage.get() {
                                        view! { <span class="muted">"—"</span> }.into_any()
                                    } else {
                                        let remove = on_remove;
                                        let id = remove_id.clone();
                                        view! {
                                            <button class="btn danger" on:click=move |_| remove(id.clone())>
                                                "Remove"
                                            </button>
                                        }.into_any()
                                    }}
                                </td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
        <div class="panel">
            <h2>"Pending invitations"</h2>
            {if invites.is_empty() {
                view! { <Empty message="No pending invitations.".to_owned()/> }.into_any()
            } else {
                view! {
                    <table class="table">
                        <thead>
                            <tr>
                                <th>"Email"</th>
                                <th>"Role"</th>
                                <th>"Created"</th>
                                <th>"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {invites.iter().map(|invite| {
                                let id = invite.id.clone();
                                let revoke = on_revoke;
                                view! {
                                    <tr>
                                        <td>{invite.email.clone()}</td>
                                        <td><span class="pill muted">{label_of(&invite.role).to_owned()}</span></td>
                                        <td class="muted">{age(invite.created_at)}</td>
                                        <td>
                                            {if can_manage.get() {
                                                view! {
                                                    <button class="btn" on:click=move |_| revoke(id.clone())>
                                                        "Revoke"
                                                    </button>
                                                }.into_any()
                                            } else {
                                                view! { <span class="muted">"—"</span> }.into_any()
                                            }}
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}
        </div>
    }
}

fn label_of(role: &str) -> &'static str {
    ROLES
        .iter()
        .find(|(value, _)| *value == role)
        .map(|(_, label)| *label)
        .unwrap_or("Viewer")
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

fn event_target_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .unwrap_or_default()
}
