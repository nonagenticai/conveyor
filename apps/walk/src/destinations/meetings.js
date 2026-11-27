// The Meetings destination (S1): the forum policies and the meetings, and the meeting room with
// its agenda, participants, positions and the five gated writes — as `apps/web` reads them.

import { el } from "../dom.js";

/** @param {any} value @returns {any[]} */
function rows(value) {
  if (Array.isArray(value?.items)) return value.items;
  return Array.isArray(value) ? value : [];
}

/** @param {any} row @param {string} key */
function text(row, key) {
  const value = row?.[key];
  return typeof value === "string" && value !== "" ? value : "—";
}

/** @param {any} row @param {string} key */
function int(row, key) {
  const value = row?.[key];
  return typeof value === "number" ? String(value) : "—";
}

/** @param {string} title @param {string[]} labels @param {Array<Array<Node|string>>} body @param {string} empty @param {string} testId */
function table(title, labels, body, empty, testId) {
  const block = el("section", {}, el("h2", { text: title }));
  if (body.length === 0) {
    block.append(el("div", { class: "empty" }, el("p", { text: empty })));
    return block;
  }
  const node = el("table", { class: "table", "data-testid": testId });
  node.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const cells of body) {
    tbody.append(
      el("tr", {}, ...cells.map((cell) => (cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) })))),
    );
  }
  node.append(tbody);
  block.append(node);
  return block;
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  return ctx.params?.id ? room(ctx, ctx.params.id) : list(ctx);
}

/** @param {any} ctx */
async function list(ctx) {
  const body = el("div", { class: "destination" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Portfolio & plan" }),
    el("h1", { text: "Meetings" }),
    el("p", { class: "lede", text: "The meetings and the forum policies they schedule from." }),
    body,
  );

  const [meetings, types] = await Promise.all([
    ctx.hasAction("meetings.list")
      ? ctx.api.invoke("meetings.list", {}).then((r) => rows(r.output))
      : Promise.resolve([]),
    ctx.hasAction("meetings.type_list")
      ? ctx.api.invoke("meetings.type_list", {}).then((r) => rows(r.output))
      : Promise.resolve([]),
  ]);

  body.append(
    table(
      "Forum policies",
      ["Type", "Version", "Decision rule"],
      types.map((row) => [
        text(row, "meeting_type_name"),
        int(row, "meeting_type_version"),
        text(row, "decision_rule"),
      ]),
      "No forum policy in this tenant yet. A meeting schedules its first version.",
      "meetings-policies",
    ),
  );
  body.append(
    table(
      "Meetings",
      ["Meeting", "Status"],
      meetings.map((row) => {
        const id = text(row, "id");
        return [
          el("a", { href: `/meetings/${id}`, text: text(row, "card_name") }),
          text(row, "status"),
        ];
      }),
      "No meeting in this tenant yet. One is scheduled from its type through meetings.schedule.",
      "meetings-list",
    ),
  );
  return { element: section, title: "Meetings" };
}

/** @param {any} ctx @param {string} id */
async function room(ctx, id) {
  const result = await ctx.api.invoke("meetings.get", { meeting_id: id });
  const detail = result.output ?? {};
  const meeting = detail.meeting ?? {};
  const agenda = rows(detail.agenda);
  const attendance = rows(detail.attendance);
  const positions = rows(detail.positions);
  const status = text(meeting, "status");

  const body = el("div", { class: "destination" });
  const actions = el("div", { class: "destination-actions" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow" }, el("a", { href: "/meetings", text: "← All meetings" })),
    el("h1", { text: text(meeting, "card_name") }),
    el("p", {
      class: "field-hint",
      text: `${text(meeting, "meeting_type_name")} · ${text(meeting, "meeting_type")} v${int(meeting, "meeting_type_version")} · ${text(meeting, "decision_rule")} · ${status}`,
    }),
    actions,
    body,
  );

  /** @param {string} name @param {string} label @param {Record<string, unknown>} input */
  function action(name, label, input) {
    return el("button", {
      class: "button",
      type: "button",
      text: label,
      onClick: () =>
        ctx.shell.openAction(name, { title: label, initial: input, onDone: () => ctx.navigate(`/meetings/${id}`) }),
    });
  }

  if (status === "scheduled" && ctx.hasAction("meetings.open")) {
    actions.append(action("meetings.open", "Open meeting", { meeting_id: id }));
  }
  if (status === "live" && ctx.hasAction("meetings.close")) {
    actions.append(action("meetings.close", "Close meeting", { meeting_id: id }));
  }
  if (ctx.hasAction("meetings.position")) {
    actions.append(action("meetings.position", "Record position", { meeting_id: id }));
  }

  const agendaBlock = el("section", {}, el("h2", { text: "Agenda" }));
  if (agenda.length === 0) {
    agendaBlock.append(
      el("div", { class: "empty" }, el("p", { text: "No item yet. The agenda materialises when the meeting opens." })),
    );
  } else {
    const listNode = el("div", { class: "destination", "data-testid": "meetings-agenda" });
    for (const item of agenda) {
      const itemId = text(item, "id");
      const open = item?.outcome === null || item?.outcome === undefined;
      const line = el("div", { class: "field-hint" },
        `${int(item, "sequence")} ${text(item, "resolved_kind")} · ${item?.outcome ?? "open"}`,
      );
      if (open && ctx.hasAction("meetings.close_item")) {
        line.append(
          el("button", {
            class: "button ghost",
            type: "button",
            text: "Close item",
            onClick: () =>
              ctx.shell.openAction("meetings.close_item", {
                title: "Close agenda item",
                initial: { meeting_id: id, agenda_item_id: itemId },
                onDone: () => ctx.navigate(`/meetings/${id}`),
              }),
          }),
        );
      }
      if (open && ctx.hasAction("meetings.defer")) {
        line.append(
          el("button", {
            class: "button ghost",
            type: "button",
            text: "Defer",
            onClick: () =>
              ctx.shell.openAction("meetings.defer", {
                title: "Defer agenda item",
                initial: { meeting_id: id, agenda_item_id: itemId, reason: "deferred from the room" },
                onDone: () => ctx.navigate(`/meetings/${id}`),
              }),
          }),
        );
      }
      listNode.append(line);
    }
    agendaBlock.append(listNode);
  }
  body.append(agendaBlock);

  body.append(
    table(
      "Participants",
      ["Seat", "Participant", "Response"],
      attendance.map((row) => [
        text(row, "seat"),
        `${text(row, "participant_kind")} · ${text(row, "participant_id")}`,
        text(row, "response_state"),
      ]),
      "No seat yet.",
      "meetings-participants",
    ),
  );
  body.append(
    table(
      "Positions",
      ["Stance", "Participant", "Surface", "Rationale"],
      positions.map((row) => [
        text(row, "stance"),
        text(row, "participant_kind"),
        text(row, "surface"),
        text(row, "rationale"),
      ]),
      "No position recorded yet.",
      "meetings-positions",
    ),
  );
  return { element: section, title: text(meeting, "card_name") };
}
