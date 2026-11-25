//! Tools — the toolkit catalogue, the grants, the call ledger and the authoring path
//! (SPEC_V2 §10.5.2; T18-4, #565).
//!
//! Reads are T13's `tools.list|get`, `toolkits.list`, `tool_bindings.list` and
//! `tool_calls.list`; writes are `tools.probe`, `tool_bindings.create|update|retire`,
//! `toolkits.publish|import_openapi|register_mcp` and `tool_authoring.*`. A binding can only
//! narrow its toolkit's effect — a ceiling above it is the server's `403 tool_effect_forbidden`,
//! shown with its code; a failed probe disables the bindings that would serve new runs, and the
//! screen renders that state rather than guessing it.

use leptos::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, fmt_int, short_id};
use crate::screens::v2_data::{int, read_rows, text};
use crate::state::AppCtx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Catalogue,
    Toolkits,
    Bindings,
    Ledger,
    Authoring,
}

const TABS: [(Tab, &str); 5] = [
    (Tab::Catalogue, "Catalogue"),
    (Tab::Toolkits, "Toolkits & probe"),
    (Tab::Bindings, "Bindings"),
    (Tab::Ledger, "Call ledger"),
    (Tab::Authoring, "Bring your own"),
];

#[derive(Clone, Debug, Default)]
struct Data {
    tools: Vec<Value>,
    toolkits: Vec<Value>,
    bindings: Vec<Value>,
    calls: Vec<Value>,
}

async fn load(ctx: AppCtx) -> Result<Data, ApiError> {
    let client = ctx.client();
    let mut data = Data::default();
    let page = json!({ "page": { "limit": 200 } });
    if ctx.has_action("tools.list") {
        data.tools = read_rows(&client, "tools.list", page.clone()).await?;
    }
    if ctx.has_action("toolkits.list") {
        data.toolkits = read_rows(&client, "toolkits.list", page.clone()).await?;
    }
    if ctx.has_action("tool_bindings.list") {
        data.bindings = read_rows(&client, "tool_bindings.list", page.clone()).await?;
    }
    if ctx.has_action("tool_calls.list") {
        data.calls = read_rows(&client, "tool_calls.list", page.clone()).await?;
    }
    Ok(data)
}

#[derive(Clone, Copy)]
struct Form {
    // probe
    probe_key: RwSignal<String>,
    // bindings
    binding_id: RwSignal<String>,
    job_binding_id: RwSignal<String>,
    run_id: RwSignal<String>,
    toolkit_id: RwSignal<String>,
    ceiling: RwSignal<String>,
    credential: RwSignal<String>,
    rate_limit: RwSignal<String>,
    binding_version: RwSignal<String>,
    enabled: RwSignal<bool>,
    // contract lookup
    contract_toolkit: RwSignal<String>,
    contract_tool: RwSignal<String>,
    contract_version: RwSignal<String>,
    // ledger filters
    ledger_run: RwSignal<String>,
    ledger_tool: RwSignal<String>,
    ledger_status: RwSignal<String>,
    // bring your own
    key: RwSignal<String>,
    title: RwSignal<String>,
    service: RwSignal<String>,
    base_url: RwSignal<String>,
    document: RwSignal<String>,
    mcp_url: RwSignal<String>,
    draft_id: RwSignal<String>,
    promote_toolkit: RwSignal<String>,
    decision: RwSignal<String>,
    note: RwSignal<String>,
}

impl Form {
    fn new() -> Self {
        Self {
            probe_key: RwSignal::new(String::new()),
            binding_id: RwSignal::new(String::new()),
            job_binding_id: RwSignal::new(String::new()),
            run_id: RwSignal::new(String::new()),
            toolkit_id: RwSignal::new(String::new()),
            ceiling: RwSignal::new("read".to_owned()),
            credential: RwSignal::new(String::new()),
            rate_limit: RwSignal::new(String::new()),
            binding_version: RwSignal::new(String::new()),
            enabled: RwSignal::new(true),
            contract_toolkit: RwSignal::new(String::new()),
            contract_tool: RwSignal::new(String::new()),
            contract_version: RwSignal::new(String::new()),
            ledger_run: RwSignal::new(String::new()),
            ledger_tool: RwSignal::new(String::new()),
            ledger_status: RwSignal::new(String::new()),
            key: RwSignal::new(String::new()),
            title: RwSignal::new(String::new()),
            service: RwSignal::new(String::new()),
            base_url: RwSignal::new(String::new()),
            document: RwSignal::new("{\n  \"openapi\": \"3.1.0\"\n}".to_owned()),
            mcp_url: RwSignal::new(String::new()),
            draft_id: RwSignal::new(String::new()),
            promote_toolkit: RwSignal::new(String::new()),
            decision: RwSignal::new("approve".to_owned()),
            note: RwSignal::new(String::new()),
        }
    }
}

fn input_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .unwrap_or_default()
}

fn area_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
        .map(|area| area.value())
        .unwrap_or_default()
}

fn select_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
        .map(|select| select.value())
        .unwrap_or_default()
}

fn error_text(error: &ApiError) -> String {
    match error {
        ApiError::Invalid { detail, errors } if !errors.is_empty() => {
            let fields: Vec<String> = errors
                .iter()
                .map(|error| format!("{}: {} [{}]", error.field, error.detail, error.code))
                .collect();
            format!("{detail} · {}", fields.join(" · "))
        }
        other => other.to_string(),
    }
}

fn run(
    ctx: AppCtx,
    action: &'static str,
    input: Value,
    notice: RwSignal<Option<String>>,
    ok: impl FnOnce(Value) + 'static,
    done: &'static str,
) {
    leptos::task::spawn_local(async move {
        match ctx.invoke(action, input, InvokeOptions::default()).await {
            Ok(invocation) => {
                notice.set(Some(done.to_owned()));
                ok(invocation.output);
            }
            Err(error) => notice.set(Some(format!("{action} refused: {}", error_text(&error)))),
        }
    });
}

fn parse_object(field: &str, text: &str, notice: RwSignal<Option<String>>) -> Option<Value> {
    match serde_json::from_str::<Value>(text) {
        Ok(value) if value.is_object() => Some(value),
        Ok(_) => {
            notice.set(Some(format!("{field} must be a JSON object")));
            None
        }
        Err(error) => {
            notice.set(Some(format!("{field} is not JSON: {error}")));
            None
        }
    }
}

#[component]
pub fn Tools() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let tab = RwSignal::new(Tab::Catalogue);
    let notice = RwSignal::new(Option::<String>::None);
    let probe = RwSignal::new(Option::<Value>::None);
    let contract = RwSignal::new(Option::<Value>::None);
    let response = RwSignal::new(Option::<Value>::None);
    let ledger = RwSignal::new(Option::<Vec<Value>>::None);
    let form = Form::new();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let refresh = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen" aria-labelledby="tools-title">
            {move || {
                if !ctx.has_action("tools.list") {
                    return view! {
                        <Planned
                            title="Tools is planned".to_owned()
                            detail="tools.list is not in this role's manifest; the screen appears when T13 lands.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1 id="tools-title">"Tools"</h1>
                            <p class="muted">
                                "the published catalogue, its versions and probes, the grants that bind a toolkit to a job or run, the call ledger and the path that brings a new tool in. A grant can only narrow; a failed probe unbinds new runs and leaves in-flight runs alone."
                            </p>
                        </div>
                        <div class="filterbar">
                            <button class="btn" on:click=move |_| refresh.run(())>"Refresh"</button>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <p class="notice" role="alert" data-testid="tools-notice">{message}</p>
                    })}
                    <div class="filterbar" role="tablist">
                        {TABS.into_iter().map(|(entry, label)| view! {
                            <button
                                class="btn small"
                                class:primary=move || tab.get() == entry
                                role="tab"
                                aria-selected=move || (tab.get() == entry).to_string()
                                on:click=move |_| tab.set(entry)
                            >
                                {label}
                            </button>
                        }).collect_view()}
                    </div>
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the tool surface…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=refresh/> }.into_any(),
                        Some(Ok(loaded)) => view! {
                            {match tab.get() {
                                Tab::Catalogue => catalogue_panel(loaded.clone(), form, ctx, notice, contract),
                                Tab::Toolkits => toolkits_panel(loaded.clone(), form, ctx, notice, probe, refresh),
                                Tab::Bindings => bindings_panel(loaded.clone(), form, ctx, notice, refresh),
                                Tab::Ledger => ledger_panel(loaded.clone(), form, ctx, notice, ledger),
                                Tab::Authoring => authoring_panel(form, ctx, notice, response),
                            }}
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

fn catalogue_panel(
    data: Data,
    form: Form,
    ctx: AppCtx,
    notice: RwSignal<Option<String>>,
    contract: RwSignal<Option<Value>>,
) -> AnyView {
    let lookup = move |_event: web_sys::MouseEvent| {
        let toolkit = form.contract_toolkit.get().trim().to_owned();
        let tool = form.contract_tool.get().trim().to_owned();
        if toolkit.is_empty() || tool.is_empty() {
            notice.set(Some(
                "A contract lookup needs the toolkit and the tool key".to_owned(),
            ));
            return;
        }
        let mut payload = json!({ "toolkit": toolkit, "tool": tool });
        let version = form.contract_version.get().trim().to_owned();
        if !version.is_empty() {
            match version.parse::<i64>() {
                Ok(version) => payload["toolkit_version"] = json!(version),
                Err(_) => {
                    notice.set(Some("The toolkit version is not a number".to_owned()));
                    return;
                }
            }
        }
        run(
            ctx,
            "tools.get",
            payload,
            notice,
            move |output| contract.set(Some(output)),
            "Contract read.",
        );
    };

    view! {
        <div style="margin-top:12px">
            {if data.tools.is_empty() {
                view! { <Empty message="No published tool in this tenant; toolkits.publish imports the embedded manifests and the Bring your own tab registers a new source.".to_owned()/> }.into_any()
            } else {
                view! {
                    <div class="tablewrap">
                        <table class="table" data-testid="tools-catalogue">
                            <thead><tr><th>"Toolkit"</th><th class="n">"Version"</th><th>"Tool"</th><th>"Effect"</th><th>"Idempotency"</th><th>"Status"</th><th>"Summary"</th></tr></thead>
                            <tbody>
                                {data.tools.iter().map(|row| view! {
                                    <tr>
                                        <td class="mono">{text(row, "toolkit")}</td>
                                        <td class="n">{int(row, "toolkit_version").to_string()}</td>
                                        <td class="mono">{text(row, "tool")}</td>
                                        <td><StatusPill status=text(row, "effect")/></td>
                                        <td>{text(row, "idempotency")}</td>
                                        <td><StatusPill status=text(row, "status")/></td>
                                        <td class="muted">{text(row, "summary")}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
            <section class="panel" style="margin-top:12px">
                <div class="ph"><span>"One tool's contract"</span><span class="muted">"the closed schemas the broker enforces"</span></div>
                <div class="grid-2">
                    <label class="field"><span>"Toolkit"</span>
                        <input class="inp" aria-label="Contract toolkit" prop:value=move || form.contract_toolkit.get()
                            on:input=move |event| form.contract_toolkit.set(input_value(&event))/></label>
                    <label class="field"><span>"Tool"</span>
                        <input class="inp" aria-label="Contract tool" prop:value=move || form.contract_tool.get()
                            on:input=move |event| form.contract_tool.set(input_value(&event))/></label>
                </div>
                <label class="field"><span>"Version (optional; newest published when empty)"</span>
                    <input class="inp" aria-label="Contract version" prop:value=move || form.contract_version.get()
                        on:input=move |event| form.contract_version.set(input_value(&event))/></label>
                <button class="btn small" on:click=lookup>"Read the contract"</button>
                {move || contract.get().map(|tool| view! {
                    <div data-testid="tool-contract">
                        <dl class="kv">
                            <dt>"Tool"</dt><dd class="mono">{format!("{}@{} · {}", text(&tool, "toolkit"), int(&tool, "toolkit_version"), text(&tool, "tool"))}</dd>
                            <dt>"Effect"</dt><dd><StatusPill status=text(&tool, "effect")/></dd>
                            <dt>"Idempotency"</dt><dd>{text(&tool, "idempotency")}</dd>
                        </dl>
                        <div class="ph"><span>"Input schema"</span></div>
                        <pre class="log">{serde_json::to_string_pretty(tool.get("input_schema").unwrap_or(&Value::Null)).unwrap_or_default()}</pre>
                        <div class="ph"><span>"Output schema"</span></div>
                        <pre class="log">{serde_json::to_string_pretty(tool.get("output_schema").unwrap_or(&Value::Null)).unwrap_or_default()}</pre>
                    </div>
                })}
            </section>
        </div>
    }
    .into_any()
}

fn toolkits_panel(
    data: Data,
    form: Form,
    ctx: AppCtx,
    notice: RwSignal<Option<String>>,
    probe: RwSignal<Option<Value>>,
    refresh: Callback<()>,
) -> AnyView {
    let probe_one = move |_event: web_sys::MouseEvent| {
        let key = form.probe_key.get().trim().to_owned();
        let payload = if key.is_empty() {
            json!({ "dry_run": false })
        } else {
            json!({ "toolkit": key, "dry_run": false })
        };
        run(
            ctx,
            "tools.probe",
            payload,
            notice,
            move |output| {
                probe.set(Some(output));
                // The probe can disable or restore bindings; the grants table must show it.
                refresh.run(());
            },
            "Probe finished.",
        );
    };
    let publish = move |_event: web_sys::MouseEvent| {
        run(
            ctx,
            "toolkits.publish",
            json!({}),
            notice,
            move |_| refresh.run(()),
            "The embedded manifests are published.",
        );
    };

    let disabled = data
        .bindings
        .iter()
        .filter(|row| row.get("enabled").and_then(Value::as_bool) == Some(false))
        .count();

    view! {
        <div style="margin-top:12px">
            <div class="filterbar">
                <label class="filter"><span>"Probe one toolkit (empty = every published toolkit)"</span>
                    <input class="inp" aria-label="Probe toolkit" prop:value=move || form.probe_key.get()
                        on:input=move |event| form.probe_key.set(input_value(&event))/></label>
                <button class="btn small" on:click=probe_one>"Probe"</button>
                <button class="btn small" on:click=publish>"Publish the embedded manifests"</button>
            </div>
            {move || probe.get().map(|out| {
                let toolkits = out.get("toolkits").and_then(Value::as_array).cloned().unwrap_or_default();
                view! {
                    <div class="tablewrap">
                        <table class="table" data-testid="tools-probe">
                            <thead><tr><th>"Toolkit"</th><th class="n">"Version"</th><th>"Service"</th><th>"Probe"</th><th class="n">"Latency"</th><th class="n">"Disabled"</th><th class="n">"Restored"</th><th>"Detail"</th></tr></thead>
                            <tbody>
                                {toolkits.into_iter().map(|row| view! {
                                    <tr>
                                        <td class="mono">{text(&row, "key")}</td>
                                        <td class="n">{int(&row, "toolkit_version").to_string()}</td>
                                        <td class="mono">{text(&row, "service")}</td>
                                        <td><StatusPill status=text(&row, "probe")/></td>
                                        <td class="n">{format!("{} ms", int(&row, "latency_ms"))}</td>
                                        <td class="n">{int(&row, "bindings_disabled").to_string()}</td>
                                        <td class="n">{int(&row, "bindings_restored").to_string()}</td>
                                        <td class="muted">{text(&row, "detail")}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }
            })}
            <p class="muted">
                "A failed probe disables the bindings that would serve NEW runs (`409 tool_not_ready` at the call); a run already past the broker's resolution finishes. A later green probe restores only the bindings a probe disabled."
            </p>
            <div class="tiles">
                <div class="tile"><div class="tk">"Toolkits"</div><div class="tv">{fmt_int(data.toolkits.len() as i64)}</div><div class="td">"versions returned"</div></div>
                <div class="tile"><div class="tk">"Disabled grants"</div><div class="tv">{fmt_int(disabled as i64)}</div><div class="td">"unbound for new runs"</div></div>
            </div>
            <div class="tablewrap">
                <table class="table">
                    <thead><tr><th>"Toolkit"</th><th class="n">"Version"</th><th>"Service"</th><th>"Title"</th><th>"Status"</th><th>"Auth"</th></tr></thead>
                    <tbody>
                        {data.toolkits.iter().map(|row| view! {
                            <tr>
                                <td class="mono">{text(row, "key")}</td>
                                <td class="n">{int(row, "toolkit_version").to_string()}</td>
                                <td class="mono">{text(row, "service")}</td>
                                <td>{text(row, "title")}</td>
                                <td><StatusPill status=text(row, "status")/></td>
                                <td>{text(row, "auth_adapter")}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </div>
    }
    .into_any()
}

fn bindings_panel(
    data: Data,
    form: Form,
    ctx: AppCtx,
    notice: RwSignal<Option<String>>,
    refresh: Callback<()>,
) -> AnyView {
    let create = move |_event: web_sys::MouseEvent| {
        let toolkit_id = form.toolkit_id.get().trim().to_owned();
        let job = form.job_binding_id.get().trim().to_owned();
        let run_id = form.run_id.get().trim().to_owned();
        if toolkit_id.is_empty()
            || (job.is_empty() && run_id.is_empty())
            || (!job.is_empty() && !run_id.is_empty())
        {
            notice.set(Some(
                "A grant needs one toolkit id and exactly one of a job binding or a run".to_owned(),
            ));
            return;
        }
        let mut payload = json!({
            "toolkit_id": toolkit_id,
            "effect_ceiling": form.ceiling.get(),
        });
        if !job.is_empty() {
            payload["job_binding_id"] = Value::String(job);
        }
        if !run_id.is_empty() {
            payload["run_id"] = Value::String(run_id);
        }
        let credential = form.credential.get().trim().to_owned();
        if !credential.is_empty() {
            payload["credential_alias"] = Value::String(credential);
        }
        let rate = form.rate_limit.get().trim().to_owned();
        if !rate.is_empty() {
            match rate.parse::<i32>() {
                Ok(rate) => payload["rate_limit_per_min"] = json!(rate),
                Err(_) => {
                    notice.set(Some("The rate limit is not a number".to_owned()));
                    return;
                }
            }
        }
        run(
            ctx,
            "tool_bindings.create",
            payload,
            notice,
            move |_| refresh.run(()),
            "Grant saved.",
        );
    };
    let update = move |_event: web_sys::MouseEvent| {
        let id = form.binding_id.get().trim().to_owned();
        if id.is_empty() {
            notice.set(Some("Name the grant to update".to_owned()));
            return;
        }
        let mut payload = json!({ "id": id, "effect_ceiling": form.ceiling.get(), "enabled": form.enabled.get() });
        let credential = form.credential.get().trim().to_owned();
        payload["credential_alias"] = Value::String(credential);
        let version = form.binding_version.get().trim().to_owned();
        let options = match version.parse::<i64>() {
            Ok(version) => InvokeOptions {
                if_match: Some(version),
                ..InvokeOptions::default()
            },
            Err(_) => InvokeOptions::default(),
        };
        leptos::task::spawn_local(async move {
            match ctx.invoke("tool_bindings.update", payload, options).await {
                Ok(_) => {
                    notice.set(Some("Grant updated; it can only narrow.".to_owned()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(format!(
                    "tool_bindings.update refused: {}",
                    error_text(&error)
                ))),
            }
        });
    };
    let retire = move |_event: web_sys::MouseEvent| {
        let id = form.binding_id.get().trim().to_owned();
        if id.is_empty() {
            notice.set(Some("Name the grant to retire".to_owned()));
            return;
        }
        // Retire is versioned (CONTRACT §3): the If-Match is the version Select filled.
        let version = form.binding_version.get().trim().to_owned();
        let options = match version.parse::<i64>() {
            Ok(version) => InvokeOptions {
                if_match: Some(version),
                ..InvokeOptions::default()
            },
            Err(_) => {
                notice.set(Some(
                    "Retiring needs the grant's version — select its row first".to_owned(),
                ));
                return;
            }
        };
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("tool_bindings.retire", json!({ "id": id }), options)
                .await
            {
                Ok(_) => {
                    notice.set(Some("Grant retired.".to_owned()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(format!(
                    "tool_bindings.retire refused: {}",
                    error_text(&error)
                ))),
            }
        });
    };

    view! {
        <div class="grid-2" style="margin-top:12px">
            <section class="panel" data-testid="tools-bindings">
                <div class="ph"><span>"Grants"</span><span class="muted">"a job or run, one toolkit, a ceiling"</span></div>
                {if data.bindings.is_empty() {
                    view! { <Empty message="No grant in this tenant; a binding names exactly one job binding or run and one published toolkit version.".to_owned()/> }.into_any()
                } else {
                    view! {
                        <div class="tablewrap">
                            <table class="table">
                                <thead><tr><th>"Grant"</th><th>"Job binding"</th><th>"Run"</th><th class="n">"Toolkit"</th><th>"Ceiling"</th><th>"Credential"</th><th>"Enabled"</th><th class="n">"Version"</th><th></th></tr></thead>
                                <tbody>
                                    {data.bindings.iter().map(|row| {
                                        let enabled = row.get("enabled").and_then(Value::as_bool).unwrap_or(false);
                                        let full_id = row.get("id").and_then(Value::as_str).unwrap_or_default().to_owned();
                                        // The full id addresses the row: `short_id` is the first eight
                                        // characters of a UUIDv7, which only change about every 65 s, so
                                        // two grants created in one window share the visible prefix.
                                        let row_id = full_id.clone();
                                        let row_version = row.get("version").and_then(Value::as_i64).unwrap_or(0);
                                        // Selecting the row loads its own values, so an update of one
                                        // field cannot silently narrow the ceiling or clear the
                                        // credential (an empty alias clears it by contract).
                                        let row_ceiling = row.get("effect_ceiling").and_then(Value::as_str).unwrap_or("read").to_owned();
                                        let row_credential = row.get("credential_alias").and_then(Value::as_str).unwrap_or("").to_owned();
                                        view! {
                                            <tr data-grant-id=row_id>
                                                <td class="mono">{short_id(row.get("id").and_then(Value::as_str))}</td>
                                                <td class="mono">{short_id(row.get("job_binding_id").and_then(Value::as_str))}</td>
                                                <td class="mono">{short_id(row.get("run_id").and_then(Value::as_str))}</td>
                                                <td class="n">{short_id(row.get("toolkit_id").and_then(Value::as_str))}</td>
                                                <td><StatusPill status=text(row, "effect_ceiling")/></td>
                                                <td class="mono">{text(row, "credential_alias")}</td>
                                                <td><StatusPill status=(if enabled { "active" } else { "disabled" }).to_owned()/></td>
                                                <td class="n">{int(row, "version").to_string()}</td>
                                                <td><button class="btn small" on:click=move |_| {
                                                    form.binding_id.set(full_id.clone());
                                                    form.binding_version.set(row_version.to_string());
                                                    form.ceiling.set(row_ceiling.clone());
                                                    form.credential.set(row_credential.clone());
                                                }>"Select"</button></td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
                <p class="muted">
                    "A grant disabled here (or by a failed probe) is unbound for new runs; an in-flight run already past the broker's resolution is not touched."
                </p>
            </section>
            <section class="panel">
                <div class="ph"><span>"Create a grant"</span><span class="muted">"the ceiling may only narrow the toolkit's effect"</span></div>
                <label class="field"><span>"Toolkit id"</span>
                    <input class="inp" aria-label="Toolkit id" prop:value=move || form.toolkit_id.get()
                        on:input=move |event| form.toolkit_id.set(input_value(&event))/></label>
                <div class="grid-2">
                    <label class="field"><span>"Job binding id"</span>
                        <input class="inp" aria-label="Job binding id" prop:value=move || form.job_binding_id.get()
                            on:input=move |event| form.job_binding_id.set(input_value(&event))/></label>
                    <label class="field"><span>"Or a run id"</span>
                        <input class="inp" aria-label="Run id" prop:value=move || form.run_id.get()
                            on:input=move |event| form.run_id.set(input_value(&event))/></label>
                </div>
                <div class="grid-2">
                    <label class="field"><span>"Effect ceiling"</span>
                        <select aria-label="Effect ceiling" prop:value=move || form.ceiling.get()
                            on:change=move |event| form.ceiling.set(select_value(&event))>
                            <option value="read">"read"</option>
                            <option value="write">"write"</option>
                            <option value="spend">"spend"</option>
                            <option value="destructive">"destructive"</option>
                        </select></label>
                    <label class="field"><span>"Credential alias"</span>
                        <input class="inp" aria-label="Credential alias" prop:value=move || form.credential.get()
                            on:input=move |event| form.credential.set(input_value(&event))/></label>
                </div>
                <label class="field"><span>"Rate limit per minute"</span>
                    <input class="inp" aria-label="Rate limit" prop:value=move || form.rate_limit.get()
                        on:input=move |event| form.rate_limit.set(input_value(&event))/></label>
                <button class="btn primary" on:click=create>"Save the binding"</button>
                <p class="muted">
                    "A ceiling above the toolkit's strongest published effect is refused `403 tool_effect_forbidden`, with the code — the form shows it unchanged."
                </p>
                <div class="ph"><span>"Update or retire a grant"</span></div>
                <div class="grid-2">
                    <label class="field"><span>"Grant id"</span>
                        <input class="inp" aria-label="Grant id" prop:value=move || form.binding_id.get()
                            on:input=move |event| form.binding_id.set(input_value(&event))/></label>
                    <label class="field"><span>"If-Match version"</span>
                        <input class="inp" aria-label="Grant version" prop:value=move || form.binding_version.get()
                            on:input=move |event| form.binding_version.set(input_value(&event))/></label>
                </div>
                <label class="field"><span>"Live?"</span>
                    <select aria-label="Grant enabled" prop:value=move || form.enabled.get().to_string()
                        on:change=move |event| form.enabled.set(select_value(&event) == "true")>
                        <option value="true">"live"</option>
                        <option value="false">"disabled"</option>
                    </select></label>
                <div class="filterbar">
                    <button class="btn small" on:click=update>"Update the grant"</button>
                    <button class="btn small" on:click=retire>"Retire the grant"</button>
                </div>
            </section>
        </div>
    }
    .into_any()
}

fn ledger_panel(
    data: Data,
    form: Form,
    ctx: AppCtx,
    notice: RwSignal<Option<String>>,
    ledger: RwSignal<Option<Vec<Value>>>,
) -> AnyView {
    let filter = move |_event: web_sys::MouseEvent| {
        // The action's filters are server-side; Apply re-reads the ledger through the action
        // rather than filtering the loaded page client-side.
        let run_id = form.ledger_run.get().trim().to_owned();
        let tool_id = form.ledger_tool.get().trim().to_owned();
        let status = form.ledger_status.get();
        let mut input = json!({ "page": { "limit": 200 } });
        if !run_id.is_empty() {
            input["run_id"] = Value::String(run_id);
        }
        if !tool_id.is_empty() {
            input["tool_id"] = Value::String(tool_id);
        }
        if !status.is_empty() {
            input["status"] = Value::String(status);
        }
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("tool_calls.list", input, InvokeOptions::default())
                .await
            {
                Ok(invocation) => {
                    let rows = invocation
                        .output
                        .get("items")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    notice.set(Some(format!("{} call(s) match.", rows.len())));
                    ledger.set(Some(rows));
                }
                Err(error) => notice.set(Some(format!(
                    "tool_calls.list refused: {}",
                    error_text(&error)
                ))),
            }
        });
    };
    let rows = move || ledger.get().unwrap_or_else(|| data.calls.clone());
    view! {
        <div style="margin-top:12px">
            <div class="filterbar">
                <label class="filter"><span>"Run id"</span>
                    <input class="inp" aria-label="Ledger run" prop:value=move || form.ledger_run.get()
                        on:input=move |event| form.ledger_run.set(input_value(&event))/></label>
                <label class="filter"><span>"Tool id"</span>
                    <input class="inp" aria-label="Ledger tool" prop:value=move || form.ledger_tool.get()
                        on:input=move |event| form.ledger_tool.set(input_value(&event))/></label>
                <label class="filter"><span>"Status"</span>
                    <select aria-label="Ledger status" on:change=move |event| form.ledger_status.set(select_value(&event))>
                        <option value="">"All"</option>
                        <option value="ok">"ok"</option>
                        <option value="precondition_failed">"precondition_failed"</option>
                        <option value="check_failed">"check_failed"</option>
                        <option value="pending">"pending"</option>
                        <option value="backend_error">"backend_error"</option>
                    </select></label>
                <button class="btn small" on:click=filter>"Apply"</button>
            </div>
            {move || if rows().is_empty() {
                view! { <Empty message="No tool call in this tenant's ledger; the broker writes one row per call after the destination answers.".to_owned()/> }.into_any()
            } else {
                view! {
                    <div class="tablewrap">
                        <table class="table" data-testid="tools-ledger">
                            <thead><tr><th>"Call"</th><th>"Run"</th><th>"Tool"</th><th>"Toolkit"</th><th class="n">"Version"</th><th>"Status"</th><th class="n">"HTTP"</th><th class="n">"Latency"</th></tr></thead>
                            <tbody>
                                {rows().iter().map(|row| view! {
                                    <tr>
                                        <td class="mono">{short_id(row.get("id").and_then(Value::as_str))}</td>
                                        <td class="mono">{short_id(row.get("run_id").and_then(Value::as_str))}</td>
                                        <td class="mono">{text(row, "tool")}</td>
                                        <td class="mono">{text(row, "toolkit")}</td>
                                        <td class="n">{int(row, "toolkit_version").to_string()}</td>
                                        <td><StatusPill status=text(row, "status")/></td>
                                        <td class="n">{int(row, "http_status").to_string()}</td>
                                        <td class="n">{format!("{} ms", int(row, "latency_ms"))}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
            <p class="muted">
                "The ledger is the one row per call: the exact tool version, the run, the HTTP status and the latency. It is also the source of the metering tool roll-up and the zero-use retirement report."
            </p>
        </div>
    }
    .into_any()
}

fn authoring_panel(
    form: Form,
    ctx: AppCtx,
    notice: RwSignal<Option<String>>,
    response: RwSignal<Option<Value>>,
) -> AnyView {
    let register = move |_event: web_sys::MouseEvent| {
        let key = form.key.get().trim().to_owned();
        let title = form.title.get().trim().to_owned();
        let url = form.mcp_url.get().trim().to_owned();
        if key.is_empty() || title.is_empty() || url.is_empty() {
            notice.set(Some(
                "A remote MCP registration needs its key, title and URL".to_owned(),
            ));
            return;
        }
        run(
            ctx,
            "toolkits.register_mcp",
            json!({ "key": key, "title": title, "url": url }),
            notice,
            move |out| response.set(Some(out)),
            "MCP source registered.",
        );
    };
    let import = move |_event: web_sys::MouseEvent| {
        let key = form.key.get().trim().to_owned();
        let title = form.title.get().trim().to_owned();
        let base_url = form.base_url.get().trim().to_owned();
        if key.is_empty() || title.is_empty() || base_url.is_empty() {
            notice.set(Some(
                "An OpenAPI import needs the key, title and base URL".to_owned(),
            ));
            return;
        }
        let Some(document) = parse_object("The OpenAPI document", &form.document.get(), notice)
        else {
            return;
        };
        let mut payload =
            json!({ "key": key, "title": title, "document": document, "base_url": base_url });
        let service = form.service.get().trim().to_owned();
        if !service.is_empty() {
            payload["service"] = Value::String(service);
        }
        run(
            ctx,
            "toolkits.import_openapi",
            payload,
            notice,
            move |out| response.set(Some(out)),
            "OpenAPI source imported.",
        );
    };
    let verb = move |action: &'static str| {
        move |_event: web_sys::MouseEvent| {
            let draft_id = form.draft_id.get().trim().to_owned();
            let payload = match action {
                // Promote names the PUBLISHED tenant toolkit, not the draft (PromoteIn).
                "tool_authoring.promote" => {
                    let toolkit = form.promote_toolkit.get().trim().to_owned();
                    if toolkit.is_empty() {
                        notice.set(Some(
                            "Promote names the published toolkit id, not the draft".to_owned(),
                        ));
                        return;
                    }
                    json!({ "toolkit_id": toolkit })
                }
                _ if draft_id.is_empty() => {
                    notice.set(Some("Name the draft".to_owned()));
                    return;
                }
                "tool_authoring.review" => {
                    json!({ "draft_id": draft_id, "decision": form.decision.get(), "note": form.note.get() })
                }
                "tool_authoring.test" => {
                    json!({ "draft_id": draft_id, "replay": { "operation": form.note.get() } })
                }
                _ => json!({ "draft_id": draft_id }),
            };
            run(
                ctx,
                action,
                payload,
                notice,
                move |out| response.set(Some(out)),
                "Authoring step ran.",
            );
        }
    };

    view! {
        <div class="grid-2" style="margin-top:12px">
            <section class="panel" data-testid="tools-byo">
                <div class="ph"><span>"Bring your own"</span><span class="muted">"T15 · import or register a source"</span></div>
                <div class="grid-2">
                    <label class="field"><span>"Key"</span>
                        <input class="inp" aria-label="BYO key" prop:value=move || form.key.get()
                            on:input=move |event| form.key.set(input_value(&event))/></label>
                    <label class="field"><span>"Title"</span>
                        <input class="inp" aria-label="BYO title" prop:value=move || form.title.get()
                            on:input=move |event| form.title.set(input_value(&event))/></label>
                </div>
                <div class="grid-2">
                    <label class="field"><span>"Service alias"</span>
                        <input class="inp" aria-label="BYO service" prop:value=move || form.service.get()
                            on:input=move |event| form.service.set(input_value(&event))/></label>
                    <label class="field"><span>"Base URL (OpenAPI)"</span>
                        <input class="inp" aria-label="BYO base URL" prop:value=move || form.base_url.get()
                            on:input=move |event| form.base_url.set(input_value(&event))/></label>
                </div>
                <label class="field"><span>"MCP endpoint (streamable HTTP)"</span>
                    <input class="inp" aria-label="BYO MCP URL" prop:value=move || form.mcp_url.get()
                        on:input=move |event| form.mcp_url.set(input_value(&event))/></label>
                <label class="field"><span>"OpenAPI document (JSON)"</span>
                    <textarea class="inp" rows="5" aria-label="OpenAPI document" prop:value=move || form.document.get()
                        on:input=move |event| form.document.set(area_value(&event))></textarea></label>
                <div class="filterbar">
                    <button class="btn small" on:click=register>"Register the MCP source"</button>
                    <button class="btn small" on:click=import>"Import the OpenAPI source"</button>
                </div>
                <p class="muted">
                    "A registered source is a draft until an authoring job drafts, validates, tests, reviews and publishes it; the broker sends only the sealed credential alias."
                </p>
            </section>
            <section class="panel">
                <div class="ph"><span>"Authoring path"</span><span class="muted">"draft → validate → test → review → publish → promote"</span></div>
                <label class="field"><span>"Draft id (validate, test, review, publish)"</span>
                    <input class="inp" aria-label="Draft id" prop:value=move || form.draft_id.get()
                        on:input=move |event| form.draft_id.set(input_value(&event))/></label>
                <label class="field"><span>"Published toolkit id (promote)"</span>
                    <input class="inp" aria-label="Promote toolkit id" prop:value=move || form.promote_toolkit.get()
                        on:input=move |event| form.promote_toolkit.set(input_value(&event))/></label>
                <label class="field"><span>"Review decision"</span>
                    <select aria-label="Review decision" prop:value=move || form.decision.get()
                        on:change=move |event| form.decision.set(select_value(&event))>
                        <option value="approve">"approve"</option>
                        <option value="reject">"reject"</option>
                    </select></label>
                <label class="field"><span>"Review note / test operation"</span>
                    <input class="inp" aria-label="Authoring note" prop:value=move || form.note.get()
                        on:input=move |event| form.note.set(input_value(&event))/></label>
                <div class="filterbar">
                    <button class="btn small" on:click=verb("tool_authoring.validate")>"Validate"</button>
                    <button class="btn small" on:click=verb("tool_authoring.test")>"Test"</button>
                    <button class="btn small" on:click=verb("tool_authoring.review")>"Review"</button>
                    <button class="btn small" on:click=verb("tool_authoring.publish")>"Publish"</button>
                    <button class="btn small" on:click=verb("tool_authoring.promote")>"Promote"</button>
                </div>
                {move || response.get().map(|out| view! {
                    <pre class="log" data-testid="tools-response">{serde_json::to_string_pretty(&out).unwrap_or_default()}</pre>
                })}
                <p class="muted">
                    "The authoring verbs are T15's: a draft carries its operations and closed schemas, validation names every finding, and publication requests its gate rather than self-approving."
                </p>
            </section>
        </div>
    }
    .into_any()
}
