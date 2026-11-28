// The Plan + Board destination (S1): the plan's projects and their WBS, baselines and registers,
// and the board's lanes and cards. Writes are the schema sheet; moves and updates carry the
// row's version as If-Match; archive is destructive and needs an approval id.

import { el } from "../dom.js";
import { streamSse } from "../api/events.js";

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
    tbody.append(el("tr", {}, ...cells.map((cell) => (cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) })))));
  }
  node.append(tbody);
  block.append(node);
  return block;
}

/** @param {any} ctx @param {HTMLElement} root @param {string} topics @param {() => void} refetch */
function subscribe(ctx, root, topics, refetch) {
  const controller = new AbortController();
  const timer = setInterval(() => {
    if (!root.isConnected) {
      controller.abort();
      clearInterval(timer);
    }
  }, 1000);
  let lastId = "";
  void (async () => {
    let backoff = 1000;
    while (!controller.signal.aborted) {
      try {
        await streamSse(`/api/events?topics=${encodeURIComponent(topics)}`, {
          headers: {
            ...ctx.api.auth.headers(),
            ...(lastId ? { "last-event-id": lastId } : {}),
          },
          signal: controller.signal,
          onFrame: (frame) => {
            if (frame.id) lastId = frame.id;
            if (
              frame.event === "resync" ||
              frame.event.startsWith("card.") ||
              frame.event.startsWith("list.")
            ) {
              refetch();
            }
          },
        });
        backoff = 1000;
      } catch {
        if (controller.signal.aborted) break;
      }
      await new Promise((resolve) => setTimeout(resolve, backoff));
      backoff = Math.min(backoff * 2, 15000);
    }
    clearInterval(timer);
  })();
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  return ctx.params?.board_id ? board(ctx, ctx.params.board_id) : plan(ctx);
}

/** @param {any} ctx */
async function plan(ctx) {
  const detail = el("div", { class: "destination" });
  const body = el("div", { class: "destination" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Portfolio & plan" }),
    el("h1", { text: "Plan" }),
    el("p", { class: "lede", text: "The tenant's projects, and the selected project's WBS, baselines and registers." }),
    body,
    detail,
  );

  async function loadDetail(projectId) {
    const [wbs, baselines, registers] = await Promise.all([
      ctx.hasAction("wbs.list")
        ? ctx.api.invoke("wbs.list", { project_id: projectId }).then((r) => rows(r.output))
        : Promise.resolve([]),
      ctx.hasAction("baselines.list")
        ? ctx.api.invoke("baselines.list", { project_id: projectId }).then((r) => rows(r.output))
        : Promise.resolve([]),
      ctx.hasAction("registers.list")
        ? ctx.api.invoke("registers.list", { project_id: projectId }).then((r) => rows(r.output))
        : Promise.resolve([]),
    ]);
    detail.replaceChildren();
    const actions = el("div", { class: "destination-actions" });
    /** @param {string} name @param {string} label */
    const button = (name, label) =>
      el("button", {
        class: "button",
        type: "button",
        text: label,
        onClick: () =>
          ctx.shell.openAction(name, {
            title: label,
            initial: { project_id: projectId },
            onDone: () => loadDetail(projectId),
          }),
      });
    if (ctx.hasAction("changes.request")) actions.append(button("changes.request", "Request change"));
    if (ctx.hasAction("wbs.create")) actions.append(button("wbs.create", "Create WBS item"));
    if (ctx.hasAction("registers.create")) {
      actions.append(button("registers.create", "Create register item"));
    }
    if (ctx.hasAction("baselines.take")) actions.append(button("baselines.take", "Take baseline"));
    detail.append(actions);
    detail.append(
      table(
        "WBS",
        ["Title", "Kind", "Status", "Version"],
        wbs.map((row) => [text(row, "title"), text(row, "kind"), text(row, "status"), int(row, "version")]),
        "This project has no WBS yet.",
        "plan-wbs",
      ),
    );
    detail.append(
      table(
        "Baselines",
        ["Name", "Status", "Version"],
        baselines.map((row) => [text(row, "name"), text(row, "status"), int(row, "version")]),
        "This project has no baseline yet.",
        "plan-baselines",
      ),
    );
    detail.append(
      table(
        "Registers",
        ["Title", "Kind", "Status"],
        registers.map((row) => [text(row, "title"), text(row, "kind"), text(row, "status")]),
        "This project has no register item yet.",
        "plan-registers",
      ),
    );
  }

  const projects = rows(await ctx.api.request("/api/projects?limit=200"));
  if (projects.length === 0) {
    body.append(el("div", { class: "empty" }, el("p", { text: "No project in this tenant yet." })));
    return { element: section, title: "Plan" };
  }
  const node = el("table", { class: "table", "data-testid": "plan-projects" });
  node.append(
    el("thead", {}, el("tr", {},
      el("th", { text: "Key" }),
      el("th", { text: "Project" }),
      el("th", { text: "Status" }),
      el("th", {}),
    )),
  );
  const tbody = el("tbody");
  for (const project of projects) {
    const id = text(project, "id");
    tbody.append(
      el("tr", {},
        el("td", { text: text(project, "key") }),
        el("td", { text: text(project, "name") }),
        el("td", { text: text(project, "status") }),
        el("td", {}, el("button", {
          class: "button ghost",
          type: "button",
          text: "Open",
          onClick: () => void loadDetail(id),
        })),
      ),
    );
  }
  node.append(tbody);
  body.append(node);
  return { element: section, title: "Plan" };
}

/** @param {any} ctx @param {string} id */
async function board(ctx, id) {
  const body = el("div", { class: "destination" });
  // Cards archived in this session. The server serves no archived-card read
  // (crates/api/src/board.rs is live-only), so this is the only honest list an Unarchive control
  // can name — the same session-state parity apps/web keeps (screens/boards.rs:544,1123).
  const archived = [];
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Portfolio & plan" }),
    el("h1", { text: "Board" }),
    body,
  );

  async function load() {
    const [view, page] = await Promise.all([
      ctx.api.request(`/api/boards/${id}`),
      ctx.api.request(`/api/boards/${id}/cards?limit=200`),
    ]);
    const lanes = rows(view?.lists).map((entry) => entry?.list ?? entry);
    const labels = rows(view?.labels);
    const fields = rows(view?.fields);
    const cards = rows(page);
    body.replaceChildren();
    section.querySelector("h1").textContent = text(view?.board, "name");
    const actions = el("div", { class: "destination-actions" });
    /** @param {string} name @param {string} label @param {Record<string, unknown>} input */
    const button = (name, label, input) =>
      el("button", {
        class: "button",
        type: "button",
        text: label,
        onClick: () => ctx.shell.openAction(name, { title: label, initial: input, onDone: load }),
      });
    if (ctx.hasAction("cards.create")) {
      actions.append(button("cards.create", "New card", { board_id: id, list_id: lanes[0]?.id }));
    }
    if (ctx.hasAction("lists.create")) {
      actions.append(button("lists.create", "New lane", { board_id: id, kind: "queue" }));
    }
    if (ctx.hasAction("labels.create")) {
      actions.append(button("labels.create", "New label", { board_id: id }));
    }
    if (ctx.hasAction("custom_fields.create")) {
      actions.append(button("custom_fields.create", "New field", { board_id: id }));
    }
    body.append(actions);

    const laneHost = el("div", { class: "destination" });
    for (const lane of lanes) {
      const laneId = text(lane, "id");
      const laneHead = el("div", { class: "lane-head" }, el("h2", { text: text(lane, "name") }));
      if (ctx.hasAction("lists.update")) {
        laneHead.append(
          el("button", {
            class: "button ghost",
            type: "button",
            text: "Rename lane",
            onClick: () =>
              ctx.shell.openAction("lists.update", {
                title: "Rename lane",
                initial: { id: laneId, name: text(lane, "name") },
                onDone: load,
              }),
          }),
        );
      }
      if (ctx.hasAction("lists.move")) {
        laneHead.append(
          el("button", {
            class: "button ghost",
            type: "button",
            text: "Move lane",
            onClick: () =>
              ctx.shell.openAction("lists.move", {
                title: "Move lane",
                initial: { id: laneId },
                onDone: load,
              }),
          }),
        );
      }
      const block = el("section", {}, laneHead);
      const laneCards = cards.filter((card) => text(card, "list_id") === laneId);
      if (laneCards.length === 0) {
        block.append(el("div", { class: "empty" }, el("p", { text: "No card in this lane." })));
      } else {
        const node = el("table", { class: "table" });
        node.append(
          el("thead", {}, el("tr", {},
            el("th", { text: "Card" }),
            el("th", { text: "Type" }),
            el("th", { text: "Status" }),
            el("th", { text: "Version" }),
            el("th", {}),
          )),
        );
        const tbody = el("tbody");
        for (const card of laneCards) {
          const cardId = text(card, "id");
          const version = card?.version;
          const cells = [
            el("td", { text: text(card, "name") }),
            el("td", { text: text(card, "card_type") }),
            el("td", { text: text(card, "status") }),
            el("td", { text: int(card, "version") }),
            el("td", {}),
          ];
          if (ctx.hasAction("cards.update")) {
            cells[4].append(
              el("button", {
                class: "button ghost",
                type: "button",
                text: "Edit",
                onClick: () =>
                  ctx.shell.openAction("cards.update", {
                    title: "Edit card",
                    initial: { id: cardId, name: text(card, "name") },
                    ifMatch: version,
                    onDone: load,
                  }),
              }),
            );
          }
          if (ctx.hasAction("cards.move")) {
            cells[4].append(
              el("button", {
                class: "button ghost",
                type: "button",
                text: "Move",
                onClick: () =>
                  ctx.shell.openAction("cards.move", {
                    title: "Move card",
                    initial: { id: cardId, list_id: laneId },
                    ifMatch: version,
                    onDone: load,
                  }),
              }),
            );
          }
          if (ctx.hasAction("cards.archive")) {
            cells[4].append(
              el("button", {
                class: "button ghost",
                type: "button",
                text: "Archive",
                onClick: () =>
                  ctx.shell.openAction("cards.archive", {
                    title: "Archive card",
                    initial: { id: cardId },
                    // The server exposes no archived-card read (api/board.rs is live-only), so an
                    // archived card is remembered for this session exactly as apps/web does
                    // (screens/boards.rs:544,1123). It is what "Unarchive" can offer.
                    onDone: () => {
                      archived.push({ id: cardId, name: text(card, "name") });
                      load();
                    },
                  }),
              }),
            );
          }
          if (labels.length > 0 && (ctx.hasAction("cards.add_label") || ctx.hasAction("cards.remove_label"))) {
            const pick = /** @type {HTMLSelectElement} */ (
              el("select", { "aria-label": `Label for ${text(card, "name")}` })
            );
            for (const label of labels) {
              pick.append(el("option", { value: text(label, "id"), text: text(label, "name") }));
            }
            cells[4].append(pick);
            for (const [name, verb] of [["cards.add_label", "Add label"], ["cards.remove_label", "Remove label"]]) {
              if (!ctx.hasAction(name)) continue;
              cells[4].append(
                el("button", {
                  class: "button ghost",
                  type: "button",
                  text: verb,
                  onClick: () =>
                    ctx.shell.openAction(name, {
                      title: verb,
                      initial: { card_id: cardId, label_id: pick.value },
                      onDone: load,
                    }),
                }),
              );
            }
          }
          if (fields.length > 0 && (ctx.hasAction("custom_fields.set_value") || ctx.hasAction("custom_fields.clear_value"))) {
            const pick = /** @type {HTMLSelectElement} */ (
              el("select", { "aria-label": `Field for ${text(card, "name")}` })
            );
            for (const field of fields) {
              pick.append(el("option", { value: text(field, "id"), text: text(field, "name") }));
            }
            cells[4].append(pick);
            if (ctx.hasAction("custom_fields.set_value")) {
              cells[4].append(
                el("button", {
                  class: "button ghost",
                  type: "button",
                  text: "Set value",
                  onClick: () =>
                    ctx.shell.openAction("custom_fields.set_value", {
                      title: "Set field value",
                      initial: { card_id: cardId, custom_field_id: pick.value },
                      onDone: load,
                    }),
                }),
              );
            }
            if (ctx.hasAction("custom_fields.clear_value")) {
              cells[4].append(
                el("button", {
                  class: "button ghost",
                  type: "button",
                  text: "Clear value",
                  onClick: () =>
                    ctx.shell.openAction("custom_fields.clear_value", {
                      title: "Clear field value",
                      initial: { card_id: cardId, custom_field_id: pick.value },
                      onDone: load,
                    }),
                }),
              );
            }
          }
          tbody.append(el("tr", {}, ...cells));
        }
        node.append(tbody);
        block.append(node);
      }
      laneHost.append(block);
    }
    if (archived.length > 0) {
      const list = el("section", { class: "destination" }, el("h2", { text: "Archived this session" }));
      for (const card of archived) {
        const row = el("div", { class: "archived-card" }, el("span", { text: card.name }));
        if (ctx.hasAction("cards.unarchive")) {
          row.append(
            el("button", {
              class: "button ghost",
              type: "button",
              text: "Unarchive",
              onClick: () =>
                ctx.shell.openAction("cards.unarchive", {
                  title: "Unarchive card",
                  initial: { id: card.id },
                  onDone: () => {
                    const at = archived.findIndex((entry) => entry.id === card.id);
                    if (at >= 0) archived.splice(at, 1);
                    load();
                  },
                }),
            }),
          );
        }
        list.append(row);
      }
      body.append(list);
    }
    body.append(laneHost);
  }

  await load();
  subscribe(ctx, section, `board:${id}`, () => void load());
  return { element: section, title: "Board" };
}
