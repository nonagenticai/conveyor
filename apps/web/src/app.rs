//! The application root: one boot, then the nine-destination shell.
//!
//! The boot is deliberately small: load `/config.json`, finish an OIDC redirect if there is
//! one, read `/api/me`, read the role-filtered manifest. Everything after that is a screen.

use leptos::prelude::*;
use leptos_router::components::{A, Route, Router, Routes};
use leptos_router::path;
use wasm_bindgen::JsCast;

use crate::api::{ApiError, Manifest};
use crate::auth;
use crate::palette::Palette;
use crate::screens::admin::AdminScreen;
use crate::screens::benchmarks::{BenchmarkRunDetail, Benchmarks};
use crate::screens::boards::{BoardIndex, BoardScreen};
use crate::screens::cockpit::Cockpit;
use crate::screens::copilot::CopilotScreen;
use crate::screens::evals::{EvalDetail, Evals};
use crate::screens::improvement::Improvement;
use crate::screens::members::Members;
use crate::screens::models::{ModelDetail, ModelsScreen};
use crate::screens::platform::Platform;
use crate::screens::process_map::{ProcessMapDetail, ProcessMapList};
use crate::screens::processes::{ProcessDetail, ProcessesScreen};
use crate::screens::prompt_audit::PromptAuditScreen;
use crate::screens::requirements::{RequirementDetail, RequirementsScreen};
use crate::screens::roles::Roles;
use crate::screens::run_detail::RunDetail;
use crate::screens::runs::RunQueue;
use crate::screens::search::SearchScreen;
use crate::screens::sign_in::SignIn;
use crate::screens::today::Today;
use crate::screens::usage::Usage;
use crate::state::AppCtx;

async fn run_boot(ctx: AppCtx) -> Result<(), ApiError> {
    let config = auth::load_config().await;
    ctx.config.set(config.clone());
    let mut token = None;
    if let Some(config) = &config {
        token = auth::complete_login(config).await?;
    }
    ctx.token.set(token);

    // A tenant persisted by an older session may no longer be a membership. Retry once
    // without it; with one membership the server resolves the row itself.
    let client = ctx.client();
    let me = match auth::load_me(&client).await {
        Ok(me) => me,
        Err(ApiError::Forbidden(_)) if ctx.tenant.get_untracked().is_some() => {
            ctx.tenant.set(None);
            crate::state::local_remove(crate::state::TENANT_KEY);
            auth::load_me(&ctx.client()).await?
        }
        Err(error) => return Err(error),
    };
    // A caller with SEVERAL memberships and none selected gets a 400 from any tenant-scoped
    // route — including the manifest. Pick one (the remembered choice is already in the
    // signal; otherwise the server's active tenant, else the first membership) so the shell
    // can boot and the switcher can change it. A super-admin is not required to be a member
    // of anything, so this is a default, never a requirement.
    if ctx.tenant.get_untracked().is_none()
        && let Some(chosen) = me
            .active_tenant
            .clone()
            .or_else(|| me.memberships.first().cloned())
    {
        crate::state::local_set(crate::state::TENANT_KEY, &chosen.tenant_id);
        ctx.tenant.set(Some(chosen.tenant_id));
    }
    ctx.me.set(Some(me));
    let manifest: Manifest = ctx.client().manifest().await?;
    ctx.manifest.set(Some(manifest));
    Ok(())
}

#[component]
pub fn App() -> impl IntoView {
    let ctx = AppCtx::new();
    provide_context(ctx);
    let boot = LocalResource::new(move || async move { run_boot(ctx).await });

    view! {
        {move || match boot.get() {
            None => view! {
                <div class="boot"><p class="muted">"Starting Conveyor…"</p></div>
            }.into_any(),
            Some(Ok(())) => view! { <Shell/> }.into_any(),
            Some(Err(ApiError::Unauthenticated)) => view! { <SignIn/> }.into_any(),
            Some(Err(error)) => view! { <Fatal error=error/> }.into_any(),
        }}
    }
}

#[component]
fn Fatal(error: ApiError) -> impl IntoView {
    view! {
        <main class="center">
            <div class="panel problem" role="alert">
                <h1>"Conveyor could not start"</h1>
                <p>{error.to_string()}</p>
            </div>
        </main>
    }
}

#[component]
fn Shell() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();

    // One switch, not twenty panels: the design's progressive-disclosure mechanism is a root
    // attribute every screen reads, not a per-screen setting.
    Effect::new(move |_| {
        let advanced = ctx.advanced.get();
        let root = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element());
        if let Some(root) = root {
            if advanced {
                let _ = root.set_attribute("data-adv", "on");
            } else {
                let _ = root.remove_attribute("data-adv");
            }
        }
    });

    view! {
        <Router>
            <div class="app">
                <TopBar/>
                <div class="layout">
                    <Sidebar/>
                    <main class="content">
                        <Routes fallback=|| view! { <p class="muted">"No such page."</p> }>
                            <Route path=path!("/") view=Today/>
                            <Route path=path!("/boards") view=BoardIndex/>
                            <Route path=path!("/board/:board_id") view=BoardScreen/>
                            <Route path=path!("/card/:card_id") view=Cockpit/>
                            <Route path=path!("/runs") view=RunQueue/>
                            <Route path=path!("/runs/:run_id") view=RunDetail/>
                            <Route path=path!("/design/requirements") view=RequirementsScreen/>
                            <Route path=path!("/design/requirements/:id") view=RequirementDetail/>
                            <Route path=path!("/design/models") view=ModelsScreen/>
                            <Route path=path!("/design/models/:model_id") view=ModelDetail/>
                            <Route path=path!("/build/processes") view=ProcessesScreen/>
                            <Route path=path!("/build/processes/:process_id") view=ProcessDetail/>
                            <Route path=path!("/build/process-map") view=ProcessMapList/>
                            <Route path=path!("/build/process-map/:map_id") view=ProcessMapDetail/>
                            <Route path=path!("/search") view=SearchScreen/>
                            <Route path=path!("/measure/usage") view=Usage/>
                            <Route path=path!("/measure/prompt-audit") view=PromptAuditScreen/>
                            <Route path=path!("/evals/benchmarks") view=Benchmarks/>
                            <Route path=path!("/evals/benchmarks/run/:run_id") view=BenchmarkRunDetail/>
                            <Route path=path!("/evals/improvement") view=Improvement/>
                            <Route path=path!("/evals") view=Evals/>
                            <Route path=path!("/evals/:eval_id") view=EvalDetail/>
                            <Route path=path!("/evals/:eval_id/:tab") view=EvalDetail/>
                            <Route path=path!("/settings/members") view=Members/>
                            <Route path=path!("/settings/roles") view=Roles/>
                            <Route path=path!("/settings/admin") view=AdminScreen/>
                            <Route path=path!("/copilot") view=CopilotScreen/>
                            <Route path=path!("/platform") view=Platform/>
                        </Routes>
                    </main>
                </div>
                <Toast/>
                <Palette/>
            </div>
        </Router>
    }
}

#[component]
fn TopBar() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();

    let on_tenant = move |event: web_sys::Event| {
        let tenant = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
            .map(|select| select.value())
            .unwrap_or_default();
        if !tenant.is_empty() {
            ctx.switch_tenant(tenant);
        }
    };
    let toggle_advanced = move |_| {
        let next = !ctx.advanced.get_untracked();
        ctx.advanced.set(next);
        crate::state::local_set(crate::state::ADVANCED_KEY, if next { "1" } else { "0" });
    };

    view! {
        <header class="topbar">
            <div class="topbar-in">
                <A href="/" exact=true>
                    <span class="brand">
                        <span class="mark">"C"</span>
                        <span>"Conveyor"<small>"the work, not the workflow"</small></span>
                    </span>
                </A>
                <div class="spacer"></div>
                <button
                    class="btn search-btn"
                    on:click=move |_| ctx.palette_open.set(true)
                    title="Search screens, objects and actions"
                >
                    "Search " <kbd>"⌘K"</kbd>
                </button>
                {move || ctx.me.get().map(|me| {
                    let selected = ctx
                        .tenant
                        .get()
                        .or_else(|| me.active_tenant.as_ref().map(|t| t.tenant_id.clone()))
                        .unwrap_or_default();
                    view! {
                        <label class="tenant-switch">
                            <span class="sr-only">"Active tenant"</span>
                            <select on:change=on_tenant>
                                {me.memberships.iter().map(|membership| {
                                    let id = membership.tenant_id.clone();
                                    let current = selected.clone();
                                    let id_for_selected = id.clone();
                                    let label = format!("{} · {}", membership.name, membership.slug);
                                    view! {
                                        <option
                                            value=id
                                            selected=move || current == id_for_selected
                                        >
                                            {label}
                                        </option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        <span class="who">
                            {me.principal.display_name}
                        </span>
                    }
                })}
                <button
                    class="toggle"
                    on:click=toggle_advanced
                    aria-pressed=move || ctx.advanced.get().to_string()
                    title="Show ids, versions, action names and raw JSON on every screen"
                >
                    <span class="switch"></span>
                    "Advanced"
                </button>
            </div>
        </header>
    }
}

#[component]
fn Sidebar() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let role = move || ctx.role().unwrap_or_else(|| "member".to_owned());
    view! {
        <nav class="rail" aria-label="Destinations">
            <ul class="rail-list">
                <li><A href="/" exact=true><span class="nav-item">"Today"</span></A></li>
                <li><A href="/boards"><span class="nav-item">"Board"</span></A></li>
                <li><A href="/runs"><span class="nav-item">"Runs"</span></A></li>
                {move || ctx.has_action("processes.list").then(|| view! {
                    <li><A href="/build/processes"><span class="nav-item">"Build"</span></A></li>
                })}
                {move || ctx.has_action("process_maps.list").then(|| view! {
                    <li><A href="/build/process-map"><span class="nav-item">"Process map"</span></A></li>
                })}
                {move || ctx.has_action("requirements.list").then(|| view! {
                    <li><A href="/design/requirements"><span class="nav-item">"Requirements"</span></A></li>
                })}
                {move || ctx.has_action("models.list").then(|| view! {
                    <li><A href="/design/models"><span class="nav-item">"Architecture"</span></A></li>
                })}
                <li><A href="/search"><span class="nav-item">"Search"</span></A></li>
                {move || ctx.has_action("members.list").then(|| view! {
                    <li><A href="/settings/members"><span class="nav-item">"Members"</span></A></li>
                })}
                {move || ctx.has_action("roles.policy_get").then(|| view! {
                    <li><A href="/settings/roles"><span class="nav-item">"Roles & permissions"</span></A></li>
                })}
                {move || (ctx.has_action("inference_usage.report")
                    && ctx.has_action("inference_calls.list")).then(|| view! {
                    <li><A href="/measure/usage"><span class="nav-item">"Usage & cache"</span></A></li>
                })}
                {move || ctx.has_action("prompt_audits.list").then(|| view! {
                    <li><A href="/measure/prompt-audit"><span class="nav-item">"Prompt audit"</span></A></li>
                })}
                {move || ctx.has_action("benchmark_suites.list").then(|| view! {
                    <li><A href="/evals/benchmarks"><span class="nav-item">"Benchmarks"</span></A></li>
                })}
                {move || ctx.has_action("benchmark_runs.trend").then(|| view! {
                    <li><A href="/evals/improvement"><span class="nav-item">"Improvement"</span></A></li>
                })}
                {move || ctx.has_action("evals.list").then(|| view! {
                    <li><A href="/evals"><span class="nav-item">"Evals"</span></A></li>
                })}
                <li><A href="/copilot"><span class="nav-item">"Copilot"</span></A></li>
                {move || ctx.has_action("actions.list").then(|| view! {
                    <li><A href="/settings/admin"><span class="nav-item">"Admin"</span></A></li>
                })}
                {move || ctx.is_super_admin().then(|| view! {
                    <li><A href="/platform"><span class="nav-item">"Platform"</span></A></li>
                })}
            </ul>
            <div class="rail-foot">
                <span class="muted mono">{move || format!("role: {}", role_label(&role()))}</span>
            </div>
        </nav>
    }
}

#[component]
fn Toast() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    view! {
        {move || ctx.toast.get().map(|message| view! {
            <div class="toast" role="status">
                <span>{message}</span>
                <button class="btn small" on:click=move |_| ctx.toast.set(None)>"Dismiss"</button>
            </div>
        })}
    }
}

/// Permission labels are the owner's: the tenant owns itself; `admin` is the tenant-admin.
pub fn role_label(role: &str) -> &'static str {
    match role {
        "owner" | "admin" => "Tenant admin",
        "member" => "Member",
        "viewer" => "Viewer",
        "kata_executor" => "Machine",
        _ => "Member",
    }
}
