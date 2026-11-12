//! Platform — the super-admin surface: every tenant, the New-tenant wizard, and the
//! recovery path for a tenant that lost all of its admins.
//!
//! Three rules the screen keeps visible:
//!
//! - **It is not a tenant screen.** A non-super-admin sees the refusal sentence, not an
//!   empty table; the nav link is hidden for them too (the shell's `is_super_admin`).
//! - **The wizard says what it is doing.** Creation is four steps — Keycloak user, provision,
//!   local rows, one set-password email — and the screen lists them with their outcome. A
//!   provisioning `partial` (e.g. the billing quota) is shown as a warning with the tenant
//!   already created, never hidden behind a green tick.
//! - **A failed step leaves nothing to guess.** The upstream's own refusal is shown, and
//!   Retry re-runs the SAME deterministic operation (the key is derived from email + slug),
//!   so a retry can never create a second tenant.
//!
//! The billing panel is the shared component; it renders "unavailable" rather than zeros.

use leptos::prelude::*;
use serde_json::json;

use crate::api::ApiError;
use crate::components::{Empty, ErrorPanel, StatusPill, age};
use crate::models::{PlatformCreated, PlatformTenant, PlatformTenantDetail};
use crate::screens::billing::BillingPanel;
use crate::state::AppCtx;

fn load_tenants(
    ctx: AppCtx,
    refresh: RwSignal<u32>,
) -> LocalResource<Result<Vec<PlatformTenant>, ApiError>> {
    LocalResource::new(move || {
        let client = ctx.client();
        let _ = refresh.get();
        async move {
            let value = client.get("/api/platform/tenants").await?;
            serde_json::from_value::<Vec<PlatformTenant>>(value)
                .map_err(|error| ApiError::Network(format!("platform tenants: {error}")))
        }
    })
}

fn load_detail(
    ctx: AppCtx,
    selected: RwSignal<Option<String>>,
    refresh: RwSignal<u32>,
) -> LocalResource<Result<Option<PlatformTenantDetail>, ApiError>> {
    LocalResource::new(move || {
        let client = ctx.client();
        let id = selected.get();
        let _ = refresh.get();
        async move {
            let Some(id) = id else {
                return Ok(None);
            };
            let value = client.get(&format!("/api/platform/tenants/{id}")).await?;
            serde_json::from_value::<PlatformTenantDetail>(value)
                .map(Some)
                .map_err(|error| ApiError::Network(format!("platform tenant: {error}")))
        }
    })
}

fn licence_label(tenant: &PlatformTenant) -> String {
    match &tenant.licence {
        None => "licence: pending".to_owned(),
        Some(licence) if licence.status == "active" => match licence.expires_at {
            Some(expires) => format!("licence: active until {}", expires.format("%Y-%m-%d")),
            None => "licence: active".to_owned(),
        },
        Some(licence) => format!("licence: {}", licence.status),
    }
}

/// The one refusal the wizard can produce from its own validation.
fn validate(name: &str, slug: &str, email: &str) -> Option<String> {
    if name.trim().is_empty() {
        return Some("A tenant needs a name.".into());
    }
    let slug = slug.trim();
    let mut chars = slug.chars();
    let first_ok = matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit());
    if !first_ok || !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Some(
            "The slug is lowercase letters, digits and hyphens, starting with a letter or digit."
                .into(),
        );
    }
    let parts: Vec<&str> = email.trim().split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || !parts[1].contains('.') {
        return Some("The first admin needs a single email address.".into());
    }
    None
}

/// Slug from a display name: lowercase, non-alphanumerics collapsed to `-`.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for character in name.trim().to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[component]
pub fn Platform() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let refresh = RwSignal::new(0u32);
    let selected = RwSignal::new(None::<String>);
    let tenants = load_tenants(ctx, refresh);
    let detail = load_detail(ctx, selected, refresh);
    let advanced = ctx.advanced;

    // The wizard.
    let wizard_open = RwSignal::new(false);
    let name = RwSignal::new(String::new());
    let slug = RwSignal::new(String::new());
    let slug_touched = RwSignal::new(false);
    let admin_email = RwSignal::new(String::new());
    let admin_name = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let problem = RwSignal::new(None::<String>);
    let outcome = RwSignal::new(None::<PlatformCreated>);

    // The recovery invite.
    let invite_open = RwSignal::new(false);
    let invite_email = RwSignal::new(String::new());
    let invite_name = RwSignal::new(String::new());
    let invite_message = RwSignal::new(None::<String>);

    if !ctx.is_super_admin() {
        return view! {
            <section class="screen">
                <header class="screen-head"><div><h1>"Platform"</h1></div></header>
                <div class="panel">
                    <p>"The Platform surface is super-admin only."</p>
                    <p class="muted">"Ask a platform administrator if you need a tenant created or an admin restored."</p>
                </div>
            </section>
        }
        .into_any();
    }

    let submit = move |_| {
        let client = ctx.client();
        let name_value = name.get_untracked();
        let slug_value = slug.get_untracked();
        let email_value = admin_email.get_untracked();
        let admin_value = admin_name.get_untracked();
        if let Some(message) = validate(&name_value, &slug_value, &email_value) {
            problem.set(Some(message));
            return;
        }
        busy.set(true);
        problem.set(None);
        outcome.set(None);
        let mut body = json!({
            "name": name_value,
            "slug": slug_value.trim(),
            "admin_email": email_value.trim(),
        });
        if !admin_value.trim().is_empty() {
            body["admin_name"] = json!(admin_value.trim());
        }
        leptos::task::spawn_local(async move {
            match client.post("/api/platform/tenants", body).await {
                Ok(value) => match serde_json::from_value::<PlatformCreated>(value) {
                    Ok(created) => {
                        outcome.set(Some(created));
                        refresh.update(|n| *n += 1);
                        name.set(String::new());
                        slug.set(String::new());
                        slug_touched.set(false);
                        admin_email.set(String::new());
                        admin_name.set(String::new());
                    }
                    Err(error) => problem.set(Some(format!("unusable answer: {error}"))),
                },
                Err(error) => problem.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    let invite_submit = move |_| {
        let Some(id) = selected.get_untracked() else {
            return;
        };
        let email_value = invite_email.get_untracked();
        if email_value.trim().is_empty() {
            invite_message.set(Some("An email is required.".into()));
            return;
        }
        let client = ctx.client();
        let mut body = json!({"email": email_value.trim()});
        if !invite_name.get_untracked().trim().is_empty() {
            body["name"] = json!(invite_name.get_untracked().trim());
        }
        invite_message.set(None);
        leptos::task::spawn_local(async move {
            match client
                .post(&format!("/api/platform/tenants/{id}/invite-admin"), body)
                .await
            {
                Ok(value) => {
                    let sent = value
                        .get("email_sent")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    invite_message.set(Some(if sent {
                        "Invite recorded; the set-password email was sent.".into()
                    } else {
                        "Invite recorded; the set-password email FAILED — the invite still binds at sign-in.".into()
                    }));
                    invite_email.set(String::new());
                    invite_name.set(String::new());
                    refresh.update(|n| *n += 1);
                }
                Err(error) => invite_message.set(Some(error.to_string())),
            }
        });
    };

    let retry = move |_| {
        let Some(id) = selected.get_untracked() else {
            return;
        };
        let client = ctx.client();
        busy.set(true);
        problem.set(None);
        leptos::task::spawn_local(async move {
            match client
                .post(&format!("/api/platform/tenants/{id}/retry"), json!({}))
                .await
            {
                Ok(value) => match serde_json::from_value::<PlatformCreated>(value) {
                    Ok(created) => {
                        outcome.set(Some(created));
                        refresh.update(|n| *n += 1);
                    }
                    Err(error) => problem.set(Some(format!("unusable answer: {error}"))),
                },
                Err(error) => problem.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="screen">
            <header class="screen-head">
                <div>
                    <h1>"Platform"</h1>
                    <p class="muted">"Tenants, provisioning and the tenant-admin recovery. Super-admin only."</p>
                </div>
                <div class="actions">
                    <button class="btn" on:click=move |_| { wizard_open.update(|open| *open = !*open); problem.set(None); outcome.set(None); }>
                        {move || if wizard_open.get() { "Close" } else { "New tenant" }}
                    </button>
                </div>
            </header>

            {move || problem.get().map(|message| view! {
                <div class="panel warn"><p>{message}</p></div>
            })}

            {move || outcome.get().map(|created| {
                let status = created.provisioning.status.clone();
                let warnings = created.provisioning.warnings.clone();
                let partial = created.provisioning.partial_failures.clone();
                let email_sent = created.email_sent;
                let tenant_id = created.tenant.id.clone();
                view! {
                    <section class="panel">
                        <header class="panel-head">
                            <div>
                                <h2>"Tenant created"</h2>
                                <p class="muted">
                                    {format!("{} · {}", created.tenant.name, created.tenant.slug)}
                                </p>
                            </div>
                            <StatusPill status=status.clone()/>
                        </header>
                        <ol class="steps">
                            <li>"Keycloak user ensured"</li>
                            <li>{format!("Provisioning: {status}")}</li>
                            <li>"Tenant, admin membership and entitlement written"</li>
                            <li>{if email_sent { "Set-password email sent".to_owned() } else { "Set-password email NOT sent".to_owned() }}</li>
                        </ol>
                        {(!warnings.is_empty()).then(|| view! {
                            <div class="panel warn">
                                <strong>"Warnings"</strong>
                                <ul>{warnings.into_iter().map(|warning| view! { <li>{warning}</li> }).collect_view()}</ul>
                            </div>
                        })}
                        {(!partial.is_empty()).then(|| view! {
                            <div class="panel warn">
                                <strong>"Material failures — the tenant exists but is not fully wired"</strong>
                                <ul>{partial.into_iter().map(|failure| view! { <li>{failure}</li> }).collect_view()}</ul>
                                <p class="muted">"Retry re-runs the same provisioning operation; use it once the upstream is healthy."</p>
                            </div>
                        })}
                        <button class="btn small" on:click=move |_| selected.set(Some(tenant_id.clone()))>
                            "Open the tenant"
                        </button>
                    </section>
                }
            })}

            {move || wizard_open.get().then(|| view! {
                <section class="panel">
                    <header class="panel-head">
                        <div>
                            <h2>"New tenant"</h2>
                            <p class="muted">"Creates the tenant, its conveyor licence and its first admin."</p>
                        </div>
                    </header>
                    <div class="form">
                        <label>
                            <span>"Tenant name"</span>
                            <input
                                type="text"
                                prop:value=move || name.get()
                                on:input=move |event| {
                                    let value = event_target_value(&event);
                                    name.set(value.clone());
                                    if !slug_touched.get_untracked() {
                                        slug.set(slugify(&value));
                                    }
                                }
                            />
                        </label>
                        <label>
                            <span>"Slug"</span>
                            <input
                                type="text"
                                prop:value=move || slug.get()
                                on:input=move |event| {
                                    slug_touched.set(true);
                                    slug.set(event_target_value(&event));
                                }
                            />
                        </label>
                        <label>
                            <span>"First admin email"</span>
                            <input
                                type="email"
                                prop:value=move || admin_email.get()
                                on:input=move |event| admin_email.set(event_target_value(&event))
                            />
                        </label>
                        <label>
                            <span>"First admin name (optional)"</span>
                            <input
                                type="text"
                                prop:value=move || admin_name.get()
                                on:input=move |event| admin_name.set(event_target_value(&event))
                            />
                        </label>
                    </div>
                    <p class="muted">
                        "Plan " <span class="mono">"standard"</span> " · quota 1,000,000 · one set-password email."
                    </p>
                    <button class="btn" disabled=move || busy.get() on:click=submit>
                        {move || if busy.get() { "Creating…" } else { "Create tenant" }}
                    </button>
                </section>
            })}

            <Suspense fallback=move || view! { <p class="muted">"Loading tenants…"</p> }>
                {move || match tenants.get() {
                    None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <ErrorPanel error=error on_retry=Callback::new(move |_| refresh.update(|n| *n += 1))/>
                    }.into_any(),
                    Some(Ok(rows)) => {
                        if rows.is_empty() {
                            view! { <Empty message="No tenants yet. Create the first one.".to_string()/> }.into_any()
                        } else {
                            view! {
                                <section class="panel">
                                    <table class="table">
                                        <thead>
                                            <tr>
                                                <th>"Tenant"</th><th>"Status"</th><th>"Members"</th>
                                                <th>"Admins"</th><th>"Licence"</th><th>"Created"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {rows.into_iter().map(|tenant| {
                                                let id = tenant.id.clone();
                                                let licence = licence_label(&tenant);
                                                let active = move || selected.get().as_deref() == Some(id.as_str());
                                                view! {
                                                    <tr
                                                        class:selected=active
                                                        style="cursor:pointer"
                                                        on:click={
                                                            let id = tenant.id.clone();
                                                            move |_| selected.set(Some(id.clone()))
                                                        }
                                                    >
                                                        <td>
                                                            <strong>{tenant.name.clone()}</strong>
                                                            <div class="muted mono">{tenant.slug.clone()}</div>
                                                        </td>
                                                        <td><StatusPill status=tenant.status.clone()/></td>
                                                        <td>{tenant.members}</td>
                                                        <td>{tenant.admins}</td>
                                                        <td>{licence}</td>
                                                        <td class="muted">{age(Some(tenant.created_at))}</td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                </section>
                            }.into_any()
                        }
                    }
                }}
            </Suspense>

            {move || selected.get().map(|_| view! {
                <Suspense fallback=move || view! { <p class="muted">"Loading tenant…"</p> }>
                    {move || match detail.get() {
                        None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| refresh.update(|n| *n += 1))/>
                        }.into_any(),
                        Some(Ok(None)) => view! { <p class="muted">"Select a tenant."</p> }.into_any(),
                        Some(Ok(Some(found))) => {
                            let tenant = found.tenant.clone();
                            let billing_tenant = tenant.id.clone();
                            let ids_tenant = tenant.clone();
                            let licence = licence_label(&tenant);
                            let status = tenant
                                .licence
                                .as_ref()
                                .map(|licence| licence.status.clone())
                                .unwrap_or_else(|| "pending".to_owned());
                            let checked = found.tenant.licence.as_ref().map(|licence| age(Some(licence.checked_at)));
                            view! {
                                <section class="panel">
                                    <header class="panel-head">
                                        <div>
                                            <h2>{tenant.name.clone()}</h2>
                                            <p class="muted mono">{tenant.slug.clone()}</p>
                                        </div>
                                        <div class="actions">
                                            <StatusPill status=status.clone()/>
                                            <button class="btn small" on:click=move |_| invite_open.update(|open| *open = !*open)>
                                                "Invite tenant admin"
                                            </button>
                                            <button class="btn small" disabled=move || busy.get() on:click=retry>
                                                "Retry provisioning"
                                            </button>
                                        </div>
                                    </header>
                                    <p class="muted">{licence}</p>
                                    {checked.map(|checked| view! {
                                        <p class="muted">"Licence checked " {checked}</p>
                                    })}

                                    <h3>"Admins"</h3>
                                    {if found.admins.is_empty() {
                                        view! { <p class="muted">"No admin — use Invite tenant admin to restore one."</p> }.into_any()
                                    } else {
                                        view! {
                                            <ul class="people">
                                                {found.admins.clone().into_iter().map(|admin| view! {
                                                    <li>
                                                        <strong>{admin.display_name.clone()}</strong>
                                                        <span class="muted">" " {admin.email.clone().unwrap_or_default()}</span>
                                                        <span class="badge">{admin.role.clone()}</span>
                                                    </li>
                                                }).collect_view()}
                                            </ul>
                                        }.into_any()
                                    }}

                                    {move || invite_open.get().then(|| view! {
                                        <div class="form">
                                            <label>
                                                <span>"Email"</span>
                                                <input type="email" prop:value=move || invite_email.get()
                                                    on:input=move |event| invite_email.set(event_target_value(&event))/>
                                            </label>
                                            <label>
                                                <span>"Name (optional)"</span>
                                                <input type="text" prop:value=move || invite_name.get()
                                                    on:input=move |event| invite_name.set(event_target_value(&event))/>
                                            </label>
                                            <button class="btn small" on:click=invite_submit>"Send invite"</button>
                                        </div>
                                    })}
                                    {move || invite_message.get().map(|message| view! {
                                        <p class="muted">{message}</p>
                                    })}

                                    {move || advanced.get().then(|| view! {
                                        <details class="advanced">
                                            <summary>"Ids and versions"</summary>
                                            <pre class="mono">{format!(
                                                "tenant id: {}\nstatus: {}\nmembers: {}\nadmins: {}\ncreated: {}",
                                                ids_tenant.id, ids_tenant.status, ids_tenant.members, ids_tenant.admins, ids_tenant.created_at
                                            )}</pre>
                                        </details>
                                    })}
                                </section>
                                <BillingPanel tenant_id=billing_tenant.clone()/>
                            }.into_any()
                        }
                    }}
                </Suspense>
            })}
        </section>
    }
    .into_any()
}
