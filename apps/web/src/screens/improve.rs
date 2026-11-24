//! Improve — the kata, the coach's path and the lesson lifecycle (SPEC_V2 §10.5.4; T18-4, #565).
//!
//! Every write is an existing action: `kata.open|get|observe|analyse|hypothesise|propose|measure|
//! adopt`, `coach.triage|remedy|escalate` and `lessons.propose|review|promote`. The kata's
//! unmeasurable refusal is the server's own `422 kata_not_measurable`, shown with its sentence —
//! the screen never pre-empts or rewrites it.
//!
//! Measured gaps, named: there is no `kata.list`, no interruption-list read and no lessons-list
//! read in the registry, so each panel takes the id its action operates on and says so. No row
//! is fabricated to fill a table.

use leptos::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, Planned, StatusPill, short_id};
use crate::state::AppCtx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Kata,
    Coach,
    Lessons,
}

const TABS: [(Tab, &str); 3] = [
    (Tab::Kata, "Kata"),
    (Tab::Coach, "Coach"),
    (Tab::Lessons, "Lessons"),
];

/// The six target surfaces `kata.open` accepts.
const SURFACES: [&str; 6] = [
    "procedure",
    "prompt",
    "knowledge",
    "tool",
    "job_config",
    "code",
];

/// The closed triage classes (`coach.triage`).
const TRIAGE_CLASSES: [&str; 5] = [
    "transient",
    "conveyor_defect",
    "job_defect",
    "project_decision",
    "external",
];

#[derive(Clone, Copy)]
struct Form {
    kata_id: RwSignal<String>,
    job_binding_id: RwSignal<String>,
    signature: RwSignal<String>,
    title: RwSignal<String>,
    surface: RwSignal<String>,
    target_ref: RwSignal<String>,
    instrument: RwSignal<String>,
    threshold: RwSignal<String>,
    interruption_id: RwSignal<String>,
    triage_class: RwSignal<String>,
    confidence: RwSignal<String>,
    narrative: RwSignal<String>,
    remedy_class: RwSignal<String>,
    remedy_detail: RwSignal<String>,
    escalate_reason: RwSignal<String>,
    statement: RwSignal<String>,
    evidence: RwSignal<String>,
    lesson_id: RwSignal<String>,
    decision: RwSignal<String>,
    note: RwSignal<String>,
}

impl Form {
    fn new() -> Self {
        Self {
            kata_id: RwSignal::new(String::new()),
            job_binding_id: RwSignal::new(String::new()),
            signature: RwSignal::new(String::new()),
            title: RwSignal::new(String::new()),
            surface: RwSignal::new("procedure".to_owned()),
            target_ref: RwSignal::new(String::new()),
            instrument: RwSignal::new(String::new()),
            threshold: RwSignal::new(String::new()),
            interruption_id: RwSignal::new(String::new()),
            triage_class: RwSignal::new("transient".to_owned()),
            confidence: RwSignal::new("0.8".to_owned()),
            narrative: RwSignal::new(String::new()),
            remedy_class: RwSignal::new(String::new()),
            remedy_detail: RwSignal::new("{}".to_owned()),
            escalate_reason: RwSignal::new(String::new()),
            statement: RwSignal::new(String::new()),
            evidence: RwSignal::new(String::new()),
            lesson_id: RwSignal::new(String::new()),
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

fn text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| "—".to_owned())
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

/// One mutation: run it, show its outcome, and hand the output back for rendering.
fn run_action(
    ctx: AppCtx,
    action: &'static str,
    input: Value,
    notice: RwSignal<Option<String>>,
    on_ok: impl FnOnce(Value) + 'static,
    done_message: &'static str,
) {
    leptos::task::spawn_local(async move {
        match ctx.invoke(action, input, InvokeOptions::default()).await {
            Ok(invocation) => {
                notice.set(Some(done_message.to_owned()));
                on_ok(invocation.output);
            }
            Err(error) => notice.set(Some(format!("{action} refused: {}", error_text(&error)))),
        }
    });
}

#[component]
pub fn Improve() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let tab = RwSignal::new(Tab::Kata);
    let notice = RwSignal::new(Option::<String>::None);
    let kata = RwSignal::new(Option::<Value>::None);
    let lesson = RwSignal::new(Option::<Value>::None);
    let interruption = RwSignal::new(Option::<Value>::None);
    let form = Form::new();

    view! {
        <section class="screen" aria-labelledby="improve-title">
            {move || {
                if !ctx.has_action("kata.open") && !ctx.has_action("coach.triage") {
                    return view! {
                        <Planned
                            title="Improve is planned".to_owned()
                            detail="kata.open and coach.triage are not in this role's manifest; the screen appears when they land.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1 id="improve-title">"Improve"</h1>
                            <p class="muted">
                                "the coach's interruption path, the kata that measures a systemic finding, and the lesson that carries it. A kata opens only with a named instrument and a success threshold; the server's refusal is shown with its own sentence."
                            </p>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <p class="notice" role="alert" data-testid="improve-notice">{message}</p>
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
                    {move || match tab.get() {
                        Tab::Kata => kata_panel(form, ctx, notice, kata),
                        Tab::Coach => coach_panel(form, ctx, notice, interruption),
                        Tab::Lessons => lessons_panel(form, ctx, notice, lesson),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

fn kata_panel(
    form: Form,
    ctx: AppCtx,
    notice: RwSignal<Option<String>>,
    kata: RwSignal<Option<Value>>,
) -> AnyView {
    let open = move |_event: web_sys::MouseEvent| {
        let binding = form.job_binding_id.get().trim().to_owned();
        let signature = form.signature.get().trim().to_owned();
        let title = form.title.get().trim().to_owned();
        let target_ref = form.target_ref.get().trim().to_owned();
        if binding.is_empty() || signature.is_empty() || title.is_empty() || target_ref.is_empty() {
            notice.set(Some(
                "A kata needs its job binding, signature, title and target reference".to_owned(),
            ));
            return;
        }
        let mut payload = json!({
            "job_binding_id": binding,
            "signature": signature,
            "title": title,
            "target_surface": form.surface.get(),
            "target_ref": target_ref,
        });
        let instrument = form.instrument.get().trim().to_owned();
        if !instrument.is_empty() {
            payload["instrument_version_id"] = Value::String(instrument);
        }
        let threshold = form.threshold.get().trim().to_owned();
        if !threshold.is_empty() {
            match threshold.parse::<f64>() {
                Ok(value) => payload["threshold"] = json!(value),
                Err(_) => {
                    notice.set(Some("The threshold is not a number".to_owned()));
                    return;
                }
            }
        }
        run_action(
            ctx,
            "kata.open",
            payload,
            notice,
            move |output| kata.set(Some(output)),
            "Kata opened; its Gets and measurement follow.",
        );
    };

    let lookup = move |_event: web_sys::MouseEvent| {
        let id = form.kata_id.get().trim().to_owned();
        if id.is_empty() {
            notice.set(Some("Name the kata id".to_owned()));
            return;
        }
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("kata.get", json!({ "id": id }), InvokeOptions::default())
                .await
            {
                Ok(invocation) => {
                    notice.set(Some("Kata read.".to_owned()));
                    kata.set(Some(invocation.output));
                }
                Err(error) => notice.set(Some(format!("kata.get refused: {}", error_text(&error)))),
            }
        });
    };

    view! {
        <div class="grid-2" style="margin-top:12px">
            <section class="panel" data-testid="improve-kata">
                <div class="ph"><span>"Open a kata"</span><span class="muted">"T5 · the instrument and threshold are required"</span></div>
                <label class="field"><span>"Job binding id"</span>
                    <input class="inp" aria-label="Job binding id" prop:value=move || form.job_binding_id.get()
                        on:input=move |event| form.job_binding_id.set(input_value(&event))/></label>
                <div class="grid-2">
                    <label class="field"><span>"Signature"</span>
                        <input class="inp" aria-label="Signature" prop:value=move || form.signature.get()
                            on:input=move |event| form.signature.set(input_value(&event))/></label>
                    <label class="field"><span>"Title"</span>
                        <input class="inp" aria-label="Kata title" prop:value=move || form.title.get()
                            on:input=move |event| form.title.set(input_value(&event))/></label>
                </div>
                <div class="grid-2">
                    <label class="field"><span>"Target surface"</span>
                        <select aria-label="Target surface" prop:value=move || form.surface.get()
                            on:change=move |event| form.surface.set(select_value(&event))>
                            {SURFACES.iter().map(|surface| view! {
                                <option value=*surface>{*surface}</option>
                            }).collect_view()}
                        </select></label>
                    <label class="field"><span>"Target reference"</span>
                        <input class="inp" aria-label="Target reference" prop:value=move || form.target_ref.get()
                            on:input=move |event| form.target_ref.set(input_value(&event))/></label>
                </div>
                <div class="grid-2">
                    <label class="field"><span>"Instrument version id (required to open)"</span>
                        <input class="inp" aria-label="Instrument version id" prop:value=move || form.instrument.get()
                            on:input=move |event| form.instrument.set(input_value(&event))/></label>
                    <label class="field"><span>"Success threshold (required to open)"</span>
                        <input class="inp" aria-label="Threshold" prop:value=move || form.threshold.get()
                            on:input=move |event| form.threshold.set(input_value(&event))/></label>
                </div>
                <button class="btn primary" on:click=open>"Open the kata"</button>
                <p class="muted">
                    "Without a named instrument and a success threshold the server refuses with 422 kata_not_measurable and names every missing piece; nothing is written."
                </p>
            </section>
            <section class="panel">
                <div class="ph"><span>"Read a kata"</span><span class="muted">"kata.get"</span></div>
                <label class="field"><span>"Kata id"</span>
                    <input class="inp" aria-label="Kata id" prop:value=move || form.kata_id.get()
                        on:input=move |event| form.kata_id.set(input_value(&event))/></label>
                <button class="btn small" on:click=lookup>"Read the kata"</button>
                {move || match kata.get() {
                    None => view! {
                        <Empty message="No kata read in this session. There is no kata.list read in the registry, so the screen takes the id the open response or the board names.".to_owned()/>
                    }.into_any(),
                    Some(view) => view! {
                        <div data-testid="kata-view">
                            <dl class="kv">
                                <dt>"Kata"</dt><dd class="mono">{short_id(Some(&text(&view, "id")))}</dd>
                                <dt>"Signature"</dt><dd class="mono">{text(&view, "signature")}</dd>
                                <dt>"Title"</dt><dd>{text(&view, "title")}</dd>
                                <dt>"Status"</dt><dd><StatusPill status=text(&view, "status")/></dd>
                                <dt>"Job binding"</dt><dd class="mono">{short_id(Some(&text(&view, "job_binding_id")))}</dd>
                                <dt>"Release pin"</dt><dd class="mono">{short_id(Some(&text(&view, "release_hash")))}</dd>
                                <dt>"Threshold"</dt><dd>{view.get("threshold").and_then(Value::as_f64).map(|value| format!("{value}")).unwrap_or_else(|| "—".to_owned())}</dd>
                                <dt>"Gets"</dt><dd>{view.get("gets").and_then(Value::as_array).map(Vec::len).unwrap_or(0).to_string()}</dd>
                            </dl>
                            {view.get("measurement").filter(|value| !value.is_null()).map(|measurement| {
                                let score = |key: &str| measurement.get(key).and_then(Value::as_f64).map(|value| format!("{value:.3}")).unwrap_or_else(|| "—".to_owned());
                                view! {
                                    <div class="callout">
                                        <strong>"Measurement"</strong>
                                        <div class="mono">{format!("{} · {} → {} · {}", text(measurement, "verdict"), score("baseline_score"), score("candidate_score"), text(measurement, "metric_key"))}</div>
                                    </div>
                                }
                            })}
                        </div>
                    }.into_any(),
                }}
            </section>
        </div>
    }
    .into_any()
}

fn coach_panel(
    form: Form,
    ctx: AppCtx,
    notice: RwSignal<Option<String>>,
    interruption: RwSignal<Option<Value>>,
) -> AnyView {
    let triage = move |_event: web_sys::MouseEvent| {
        let id = form.interruption_id.get().trim().to_owned();
        if id.is_empty() {
            notice.set(Some("Name the interruption".to_owned()));
            return;
        }
        let confidence = form.confidence.get().trim().parse::<f64>().unwrap_or(0.0);
        run_action(
            ctx,
            "coach.triage",
            json!({
                "interruption_id": id,
                "triage_class": form.triage_class.get(),
                "confidence": confidence,
                "narrative": form.narrative.get(),
            }),
            notice,
            move |output| interruption.set(Some(output)),
            "Triage recorded.",
        );
    };
    let remedy = move |_event: web_sys::MouseEvent| {
        let id = form.interruption_id.get().trim().to_owned();
        if id.is_empty() {
            notice.set(Some("Name the interruption".to_owned()));
            return;
        }
        let detail = form.remedy_detail.get();
        let detail = match serde_json::from_str::<Value>(&detail) {
            Ok(value) if value.is_object() => value,
            _ => {
                notice.set(Some("The remedy detail must be a JSON object".to_owned()));
                return;
            }
        };
        let mut payload = json!({ "interruption_id": id, "detail": detail });
        let class = form.remedy_class.get().trim().to_owned();
        if !class.is_empty() {
            payload["remedy_class"] = Value::String(class);
        }
        run_action(
            ctx,
            "coach.remedy",
            payload,
            notice,
            move |output| interruption.set(Some(output)),
            "Remedy recorded.",
        );
    };
    let escalate = move |_event: web_sys::MouseEvent| {
        let id = form.interruption_id.get().trim().to_owned();
        let reason = form.escalate_reason.get().trim().to_owned();
        if id.is_empty() || reason.is_empty() {
            notice.set(Some(
                "An escalation needs the interruption and a reason".to_owned(),
            ));
            return;
        }
        run_action(
            ctx,
            "coach.escalate",
            json!({ "interruption_id": id, "reason": reason }),
            notice,
            move |output| interruption.set(Some(output)),
            "Escalated to a human approval.",
        );
    };

    view! {
        <div class="grid-2" style="margin-top:12px">
            <section class="panel" data-testid="improve-coach">
                <div class="ph"><span>"The coach path"</span><span class="muted">"T4 · triage, remedy, escalate"</span></div>
                <label class="field"><span>"Interruption id"</span>
                    <input class="inp" aria-label="Interruption id" prop:value=move || form.interruption_id.get()
                        on:input=move |event| form.interruption_id.set(input_value(&event))/></label>
                <div class="grid-2">
                    <label class="field"><span>"Triage class"</span>
                        <select aria-label="Triage class" prop:value=move || form.triage_class.get()
                            on:change=move |event| form.triage_class.set(select_value(&event))>
                            {TRIAGE_CLASSES.iter().map(|class| view! {
                                <option value=*class>{*class}</option>
                            }).collect_view()}
                        </select></label>
                    <label class="field"><span>"Confidence (0..=1)"</span>
                        <input class="inp" aria-label="Confidence" prop:value=move || form.confidence.get()
                            on:input=move |event| form.confidence.set(input_value(&event))/></label>
                </div>
                <label class="field"><span>"Diagnosis narrative"</span>
                    <textarea class="inp" rows="3" prop:value=move || form.narrative.get()
                        on:input=move |event| form.narrative.set(area_value(&event))></textarea></label>
                <button class="btn small" on:click=triage>"Triage"</button>
                <div class="grid-2">
                    <label class="field"><span>"Remedy class (optional)"</span>
                        <input class="inp" aria-label="Remedy class" prop:value=move || form.remedy_class.get()
                            on:input=move |event| form.remedy_class.set(input_value(&event))/></label>
                    <label class="field"><span>"Remedy detail (JSON)"</span>
                        <input class="inp" aria-label="Remedy detail" prop:value=move || form.remedy_detail.get()
                            on:input=move |event| form.remedy_detail.set(input_value(&event))/></label>
                </div>
                <button class="btn small" on:click=remedy>"Remedy"</button>
                <label class="field"><span>"Why a human is needed"</span>
                    <input class="inp" aria-label="Escalation reason" prop:value=move || form.escalate_reason.get()
                        on:input=move |event| form.escalate_reason.set(input_value(&event))/></label>
                <button class="btn small" on:click=escalate>"Escalate"</button>
                <p class="muted">
                    "There is no interruption-list read: the interruption is the coach card `coach.interrupt` opened on the failed card's board, and its id is what the card carries."
                </p>
            </section>
            <section class="panel">
                <div class="ph"><span>"Last response"</span></div>
                {move || match interruption.get() {
                    None => view! { <Empty message="No coach action in this session; the form's action returns the interruption row it wrote.".to_owned()/> }.into_any(),
                    Some(row) => view! {
                        <dl class="kv" data-testid="interruption-view">
                            <dt>"Interruption"</dt><dd class="mono">{short_id(Some(&text(&row, "id")))}</dd>
                            <dt>"Coach card"</dt><dd class="mono">{short_id(Some(&text(&row, "coach_card_id")))}</dd>
                            <dt>"Class"</dt><dd>{text(&row, "triage_class")}</dd>
                            <dt>"State"</dt><dd><StatusPill status=text(&row, "state")/></dd>
                        </dl>
                    }.into_any(),
                }}
            </section>
        </div>
    }
    .into_any()
}

fn lessons_panel(
    form: Form,
    ctx: AppCtx,
    notice: RwSignal<Option<String>>,
    lesson: RwSignal<Option<Value>>,
) -> AnyView {
    let propose = move |_event: web_sys::MouseEvent| {
        let statement = form.statement.get().trim().to_owned();
        if statement.is_empty() {
            notice.set(Some("A lesson needs its statement".to_owned()));
            return;
        }
        let evidence: Vec<Value> = form
            .evidence
            .get()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let (kind, id) = line.split_once(':')?;
                Some(json!({ "kind": kind.trim(), "id": id.trim() }))
            })
            .collect();
        if evidence.len() < 2 {
            notice.set(Some(
                "A lesson needs at least two distinct evidence refs (`kind:id` per line)"
                    .to_owned(),
            ));
            return;
        }
        run_action(
            ctx,
            "lessons.propose",
            json!({ "statement": statement, "evidence": evidence }),
            notice,
            move |output| lesson.set(Some(output)),
            "Lesson proposed.",
        );
    };
    let review = move |_event: web_sys::MouseEvent| {
        let id = form.lesson_id.get().trim().to_owned();
        if id.is_empty() {
            notice.set(Some("Name the lesson".to_owned()));
            return;
        }
        run_action(
            ctx,
            "lessons.review",
            json!({
                "lesson_id": id,
                "decision": form.decision.get(),
                "note": form.note.get(),
            }),
            notice,
            move |output| lesson.set(Some(output)),
            "Review recorded.",
        );
    };
    let promote = move |_event: web_sys::MouseEvent| {
        let id = form.lesson_id.get().trim().to_owned();
        if id.is_empty() {
            notice.set(Some("Name the lesson".to_owned()));
            return;
        }
        run_action(
            ctx,
            "lessons.promote",
            json!({ "lesson_id": id }),
            notice,
            move |output| lesson.set(Some(output)),
            "Lesson promoted to a source draft.",
        );
    };

    view! {
        <div class="grid-2" style="margin-top:12px">
            <section class="panel" data-testid="improve-lessons">
                <div class="ph"><span>"Propose a lesson"</span><span class="muted">"T8 · two independent refs"</span></div>
                <label class="field"><span>"Statement"</span>
                    <textarea class="inp" rows="3" aria-label="Lesson statement" prop:value=move || form.statement.get()
                        on:input=move |event| form.statement.set(area_value(&event))></textarea></label>
                <label class="field"><span>"Evidence — one `kind:id` per line (at least two)"</span>
                    <textarea class="inp" rows="3" aria-label="Lesson evidence" placeholder="card:00000000-0000-0000-0000-000000000000"
                        prop:value=move || form.evidence.get()
                        on:input=move |event| form.evidence.set(area_value(&event))></textarea></label>
                <button class="btn primary" on:click=propose>"Propose the lesson"</button>
                <div class="grid-2">
                    <label class="field"><span>"Lesson id"</span>
                        <input class="inp" aria-label="Lesson id" prop:value=move || form.lesson_id.get()
                            on:input=move |event| form.lesson_id.set(input_value(&event))/></label>
                    <label class="field"><span>"Decision"</span>
                        <select aria-label="Lesson decision" prop:value=move || form.decision.get()
                            on:change=move |event| form.decision.set(select_value(&event))>
                            <option value="approve">"approve"</option>
                            <option value="reject">"reject"</option>
                        </select></label>
                </div>
                <label class="field"><span>"Review note"</span>
                    <input class="inp" aria-label="Review note" prop:value=move || form.note.get()
                        on:input=move |event| form.note.set(input_value(&event))/></label>
                <div class="filterbar">
                    <button class="btn small" on:click=review>"Record the review"</button>
                    <button class="btn small" on:click=promote>"Promote"</button>
                </div>
                <p class="muted">
                    "There is no lessons-list read in the registry; the response of the write is the row this panel renders."
                </p>
            </section>
            <section class="panel">
                <div class="ph"><span>"Last response"</span></div>
                {move || match lesson.get() {
                    None => view! { <Empty message="No lesson action in this session; propose one, or name an id to review or promote.".to_owned()/> }.into_any(),
                    Some(row) => view! {
                        <dl class="kv" data-testid="lesson-view">
                            <dt>"Lesson"</dt><dd class="mono">{short_id(Some(&text(&row, "id")))}</dd>
                            <dt>"Statement"</dt><dd>{text(&row, "statement")}</dd>
                            <dt>"State"</dt><dd><StatusPill status=text(&row, "state")/></dd>
                            <dt>"Independent refs"</dt><dd>{row.get("independent_count").and_then(Value::as_i64).unwrap_or(0).to_string()}</dd>
                        </dl>
                    }.into_any(),
                }}
            </section>
        </div>
    }
    .into_any()
}
