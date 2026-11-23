//! Meetings — the decision forum (T19, SPEC_V2 §6.8, §10.5, D66).
//!
//! Two screens: the destination (my invitations, the forum's history) and the room (agenda,
//! evidence, participants, live positions, the running outcome and the instruments). A control
//! is rendered only when its action is in the manifest, and the server's refusal is shown
//! verbatim — an unseated owner pressing Approve sees `meeting_not_permitted`, not a dead button.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, Client, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, StatusPill};
use crate::state::AppCtx;

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

#[derive(Clone, Debug, Default, Deserialize)]
struct MeetingRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    card_name: String,
    #[serde(default)]
    meeting_type: String,
    #[serde(default)]
    meeting_type_name: String,
    #[serde(default)]
    decision_rule: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    opens_at: Option<String>,
    #[serde(default)]
    version: i64,
    #[serde(default)]
    retirement_candidate: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct AgendaRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    sequence: i32,
    #[serde(default)]
    resolved_kind: String,
    #[serde(default)]
    resolved_id: String,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    defer_reason: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct AttendanceRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    participant_kind: String,
    #[serde(default)]
    participant_id: String,
    #[serde(default)]
    seat: String,
    #[serde(default)]
    response_state: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct PositionRow {
    #[serde(default)]
    participant_kind: String,
    #[serde(default)]
    participant_id: String,
    #[serde(default)]
    stance: String,
    #[serde(default)]
    rationale: String,
    #[serde(default)]
    surface: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct InstrumentsRow {
    #[serde(default)]
    decisions: i64,
    #[serde(default)]
    items: i64,
    #[serde(default)]
    agenda_conversion: f64,
    #[serde(default)]
    quorum_miss_rate: f64,
    #[serde(default)]
    commitment_slippage: i64,
    #[serde(default)]
    action_closure: f64,
    #[serde(default)]
    participant_hours: f64,
    #[serde(default)]
    person_hours_per_decision: f64,
    #[serde(default)]
    decisions_per_meeting: f64,
    #[serde(default)]
    open_items: i64,
    #[serde(default)]
    decision_latency_seconds: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct DetailRow {
    #[serde(default)]
    meeting: MeetingRow,
    #[serde(default)]
    agenda: Vec<AgendaRow>,
    #[serde(default)]
    attendance: Vec<AttendanceRow>,
    #[serde(default)]
    positions: Vec<PositionRow>,
    #[serde(default)]
    instruments: InstrumentsRow,
}

fn run_action(
    client: Client,
    action: &'static str,
    input: Value,
    refresh: RwSignal<u32>,
    set_notice: WriteSignal<String>,
) {
    leptos::task::spawn_local(async move {
        match client.invoke(action, input, InvokeOptions::default()).await {
            Ok(_) => {
                set_notice.set(format!("{action}: done"));
                refresh.update(|value| *value += 1);
            }
            Err(error) => set_notice.set(format!("Refused · {error}")),
        }
    });
}

#[component]
pub fn MeetingsScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let refresh = RwSignal::new(0_u32);
    let rows = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            parse::<Page<MeetingRow>>(
                client
                    .invoke("meetings.list", json!({}), InvokeOptions::default())
                    .await?
                    .output,
                "meetings",
            )
        }
    });
    let retry = Callback::new(move |_| rows.refetch());

    view! {
        <section class="screen">
            <header class="screen-head">
                <h1>"Meetings"</h1>
                <p class="muted">
                    "The decision forum: agenda items resolve existing approvals, the seats"
                    " record positions, and the outcome is the arithmetic."
                </p>
            </header>
            <Suspense fallback=move || view! { <p class="muted">"Reading…"</p> }>
                {move || rows.get().map(|result| match result {
                    Err(error) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                    Ok(page) if page.items.is_empty() => view! {
                        <Empty message="No meeting in this tenant yet. One is scheduled from its type through meetings.schedule.".to_owned()/>
                    }.into_any(),
                    Ok(page) => view! {
                        <ul class="list">
                            {page.items.into_iter().map(|meeting| {
                                let href = format!("/meetings/{}", meeting.id);
                                let meta = format!(
                                    "{} · {} · {}",
                                    meeting.meeting_type_name,
                                    meeting.decision_rule,
                                    meeting.opens_at.clone().unwrap_or_else(|| "not opened".to_owned()),
                                );
                                let retirement = meeting.retirement_candidate;
                                view! {
                                    <li class="row">
                                        <a href=href>{meeting.card_name.clone()}</a>
                                        <span class="muted">{meta}</span>
                                        <StatusPill status=meeting.status.clone()/>
                                        {retirement.then(move || view! {
                                            <span class="pill wait">"no decision — retirement candidate"</span>
                                        })}
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                })}
            </Suspense>
        </section>
    }
}

#[component]
pub fn MeetingRoomScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let params = use_params_map();
    let refresh = RwSignal::new(0_u32);
    let (notice, set_notice) = signal(String::new());
    let meeting_id = move || params.get().get("id");
    let detail = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let id = meeting_id();
        let client = ctx.client();
        async move {
            let id = id.ok_or_else(|| ApiError::NotFound("no meeting in the route".to_owned()))?;
            parse::<DetailRow>(
                client
                    .invoke(
                        "meetings.get",
                        json!({ "meeting_id": id }),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "the meeting",
            )
        }
    });
    let retry = Callback::new(move |_| detail.refetch());
    let client = ctx.client();

    view! {
        <section class="screen">
            <Suspense fallback=move || view! { <p class="muted">"Reading…"</p> }>
                {move || detail.get().map(|result| match result {
                    Err(error) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                    Ok(detail) => {
                        let meeting = detail.meeting.clone();
                        let id = meeting.id.clone();
                        let scheduled = meeting.status == "scheduled";
                        let live = meeting.status == "live";
                        let open_items = detail.instruments.open_items;
                        let can_open = ctx.has_action("meetings.open");
                        let can_close = ctx.has_action("meetings.close");
                        let can_close_item = ctx.has_action("meetings.close_item");
                        let can_defer = ctx.has_action("meetings.defer");
                        let can_position = ctx.has_action("meetings.position");
                        let instruments = detail.instruments.clone();
                        let agenda = detail.agenda.clone();
                        let attendance = detail.attendance.clone();
                        let positions = detail.positions.clone();
                        let first_item = agenda.first().map(|item| item.id.clone());
                        let id_open = id.clone();
                        let id_close = id.clone();
                        let client_open = client.clone();
                        let client_close = client.clone();
                        let client_item = client.clone();
                        let client_defer = client.clone();
                        let client_position = client.clone();
                        let set_notice_open = set_notice;
                        let set_notice_close = set_notice;
                        let set_notice_item = set_notice;
                        let set_notice_defer = set_notice;
                        let set_notice_position = set_notice;
                        view! {
                            <header class="screen-head">
                                <h1>{meeting.card_name.clone()}</h1>
                                <p class="muted">
                                    {format!(
                                        "{} · {} · {}",
                                        meeting.meeting_type_name, meeting.decision_rule, meeting.status
                                    )}
                                </p>
                            </header>
                            {(!notice.get().is_empty()).then(move || view! { <p class="notice">{notice.get()}</p> })}
                            <div class="toolbar">
                                {scheduled.then(move || view! {
                                    <button class="btn" disabled=!can_open on:click=move |_| {
                                        run_action(client_open.clone(), "meetings.open", json!({"meeting_id": id_open.clone()}), refresh, set_notice_open);
                                    }>"Open meeting"</button>
                                })}
                                {live.then(move || view! {
                                    <button class="btn" disabled=!can_close on:click=move |_| {
                                        run_action(client_close.clone(), "meetings.close", json!({"meeting_id": id_close.clone()}), refresh, set_notice_close);
                                    }>"Close meeting"</button>
                                })}
                            </div>
                            <h2>"Agenda"</h2>
                            {if agenda.is_empty() {
                                view! { <p class="muted">"No item yet. The agenda materialises when the meeting opens."</p> }.into_any()
                            } else {
                                view! {
                                    <ul class="list">
                                        {agenda.iter().map(|item| {
                                            let item_id = item.id.clone();
                                            let item_id_defer = item.id.clone();
                                            let id_item = id.clone();
                                            let id_defer = id.clone();
                                            let client_item = client_item.clone();
                                            let client_defer = client_defer.clone();
                                            let set_item = set_notice_item;
                                            let set_defer = set_notice_defer;
                                            let open = item.outcome.is_none();
                                            let label = format!(
                                                "{} {} · {}",
                                                item.sequence,
                                                item.resolved_kind,
                                                item.outcome.clone().unwrap_or_else(|| "open".to_owned()),
                                            );
                                            let reason = item.defer_reason.clone();
                                            view! {
                                                <li class="row">
                                                    <span>{label}</span>
                                                    <span class="muted">{item.resolved_id.clone()}</span>
                                                    {reason.map(|reason| view! { <span class="pill wait">{reason}</span> })}
                                                    {open.then(move || view! {
                                                        <button class="btn" disabled=!can_close_item on:click=move |_| {
                                                            run_action(client_item.clone(), "meetings.close_item", json!({"meeting_id": id_item.clone(), "agenda_item_id": item_id.clone()}), refresh, set_item);
                                                        }>"Close item"</button>
                                                    })}
                                                    {open.then(move || view! {
                                                        <button class="btn" disabled=!can_defer on:click=move |_| {
                                                            run_action(client_defer.clone(), "meetings.defer", json!({"meeting_id": id_defer.clone(), "agenda_item_id": item_id_defer.clone(), "reason": "deferred from the room"}), refresh, set_defer);
                                                        }>"Defer"</button>
                                                    })}
                                                </li>
                                            }
                                        }).collect_view()}
                                    </ul>
                                }.into_any()
                            }}
                            <h2>"Participants"</h2>
                            {if attendance.is_empty() {
                                view! { <p class="muted">"No seat yet."</p> }.into_any()
                            } else {
                                view! {
                                    <ul class="list">
                                        {attendance.iter().map(|seat| view! {
                                            <li class="row">
                                                <span>{format!("{} · {}", seat.seat, seat.participant_kind)}</span>
                                                <span class="muted">{seat.participant_id.clone()}</span>
                                                <StatusPill status=seat.response_state.clone()/>
                                            </li>
                                        }).collect_view()}
                                    </ul>
                                }.into_any()
                            }}
                            <h2>"Positions"</h2>
                            {if positions.is_empty() {
                                view! { <p class="muted">"No position recorded yet."</p> }.into_any()
                            } else {
                                view! {
                                    <ul class="list">
                                        {positions.iter().map(|position| view! {
                                            <li class="row">
                                                <span>{position.stance.clone()}</span>
                                                <span class="muted">{format!("{} · {}", position.participant_kind, position.surface)}</span>
                                                <span>{position.rationale.clone()}</span>
                                            </li>
                                        }).collect_view()}
                                    </ul>
                                }.into_any()
                            }}
                            <h2>"Outcome instruments"</h2>
                            <ul class="list">
                                <li class="row"><span>"Decisions"</span><span>{format!("{} of {} items", instruments.decisions, instruments.items)}</span></li>
                                <li class="row"><span>"Agenda conversion"</span><span>{format!("{:.0}%", instruments.agenda_conversion * 100.0)}</span></li>
                                <li class="row"><span>"Quorum-miss rate"</span><span>{format!("{:.0}%", instruments.quorum_miss_rate * 100.0)}</span></li>
                                <li class="row"><span>"Decision latency"</span><span>{instruments.decision_latency_seconds.map(|seconds| format!("{seconds}s")).unwrap_or_else(|| "—".to_owned())}</span></li>
                                <li class="row"><span>"Commitment slippage"</span><span>{instruments.commitment_slippage.to_string()}</span></li>
                                <li class="row"><span>"Action closure"</span><span>{format!("{:.0}%", instruments.action_closure * 100.0)}</span></li>
                                <li class="row"><span>"Participant-hours"</span><span>{format!("{:.1}", instruments.participant_hours)}</span></li>
                                <li class="row"><span>"Person-hours per decision"</span><span>{format!("{:.1}", instruments.person_hours_per_decision)}</span></li>
                                <li class="row"><span>"Decisions per meeting"</span><span>{format!("{:.0}", instruments.decisions_per_meeting)}</span></li>
                                <li class="row"><span>"Open items"</span><span>{open_items.to_string()}</span></li>
                            </ul>
                            // The owner's own seat: the action defaults the participant to the caller.
                            {live.then(move || {
                                let Some(first_item) = first_item else { return view! { <span/> }.into_any() };
                                let id = id.clone();
                                let client = client_position.clone();
                                view! {
                                    <div class="toolbar">
                                        <button class="btn" disabled=!can_position on:click=move |_| {
                                            run_action(client.clone(), "meetings.position", json!({
                                                "meeting_id": id.clone(),
                                                "agenda_item_id": first_item.clone(),
                                                "stance": "approve",
                                                "surface": "web",
                                            }), refresh, set_notice_position);
                                        }>"Approve the first item"</button>
                                    </div>
                                }.into_any()
                            })}
                        }.into_any()
                    }
                })}
            </Suspense>
        </section>
    }
}
