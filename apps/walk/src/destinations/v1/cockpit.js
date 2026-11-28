// The card cockpit (S4, retained V1): one canonical route per card. It reads the card, its runs
// and its approvals, and the tenant's policy per gate. Every write is the schema sheet;
// cards.update and cards.claim carry the card's version as If-Match, and a 412 refetches rather
// than guessing the new version. The app chains runs.create -> runs.start behind one control; the
// walk exposes the two writes separately so no write hides behind another.

import { el } from "../../dom.js";
import { streamSse } from "../../api/events.js";

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

/** @param {any} value @returns {string} */
function short(value) {
  return typeof value === "string" && value !== "" ? value.slice(0, 8) : "—";
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
              frame.event.startsWith("run.") ||
              frame.event.startsWith("approval.")
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
  const id = ctx.params?.card_id;
  const body = el("div", { class: "destination" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow" }, el("a", { href: "/boards", text: "← Boards" })),
    el("h1", { text: "Card" }),
    body,
  );

  async function load() {
    if (typeof id !== "string" || id === "") throw new Error("no card in the route");
    const view = await ctx.api.request(`/api/cards/${encodeURIComponent(id)}`);
    const card = view?.card ?? {};
    const labels = Array.isArray(view?.labels) ? view.labels : [];
    const runs = ctx.hasAction("runs.list")
      ? rows((await ctx.api.invoke("runs.list", { card_id: id, limit: 50 })).output)
      : [];
    const approvals = ctx.hasAction("approvals.list")
      ? rows((await ctx.api.invoke("approvals.list", { card_id: id, limit: 50 })).output)
      : [];

    /** @type {Map<string, [string, string]>} */
    const policies = new Map();
    if (ctx.hasAction("approval_policies.get")) {
      const gates = [...new Set(approvals.map((approval) => text(approval, "gate")))];
      await Promise.all(
        gates.map(async (gate) => {
          const pair = await ctx.api
            .invoke("approval_policies.get", { action: gate })
            .then((result) => [
              typeof result.output?.approval === "string" ? result.output.approval : "required",
              typeof result.output?.self_approval === "string"
                ? result.output.self_approval
                : "forbidden",
            ])
            .catch(() => ["required", "forbidden"]);
          policies.set(gate, /** @type {[string, string]} */ (pair));
        }),
      );
    }

    body.replaceChildren();
    section.querySelector("h1").textContent = text(card, "name");
    const me = ctx.auth?.me?.principal?.id ?? "";
    const assignee = typeof card?.assignee_id === "string" ? card.assignee_id : null;
    const version = card?.version;

    const meta = el("p", {
      class: "field-hint",
      text: `${text(card, "card_type_code")} · list ${short(card?.list_id)} · ${text(card, "status")} · version ${typeof version === "number" ? version : "—"}`,
    });

    const actions = el("div", { class: "destination-actions" });
    if (ctx.hasAction("cards.update")) {
      actions.append(
        el("button", {
          class: "button",
          type: "button",
          text: "Edit",
          onClick: () =>
            ctx.shell.openAction("cards.update", {
              title: "Edit card",
              initial: { id: card?.id, name: text(card, "name"), description: card?.description ?? "" },
              ifMatch: version,
              onDone: load,
            }),
        }),
      );
    }
    if (assignee !== null && assignee === me && ctx.hasAction("cards.release")) {
      actions.append(
        el("button", {
          class: "button",
          type: "button",
          text: "Release",
          onClick: () =>
            ctx.shell.openAction("cards.release", {
              title: "Release card",
              initial: { card_id: card?.id },
              onDone: load,
            }),
        }),
      );
    } else if (ctx.hasAction("cards.claim")) {
      actions.append(
        el("button", {
          class: "button",
          type: "button",
          text: "Claim",
          onClick: () =>
            ctx.shell.openAction("cards.claim", {
              title: "Claim card",
              initial: { card_id: card?.id },
              ifMatch: version,
              onDone: load,
            }),
        }),
      );
    }
    if (ctx.hasAction("runs.create")) {
      actions.append(
        el("button", {
          class: "button",
          type: "button",
          text: "New run",
          onClick: () =>
            ctx.shell.openAction("runs.create", {
              title: "New run",
              initial: { card_id: card?.id },
              onDone: load,
            }),
        }),
      );
    }

    const cardPanel = el(
      "section",
      {},
      el("h2", { text: "The card" }),
      el("p", {
        class: card?.description ? "card-description" : "field-hint",
        text: card?.description ? String(card.description) : "No description yet.",
      }),
    );
    if (labels.length > 0) {
      cardPanel.append(
        el("p", { class: "labels" }, ...labels.map((label) => el("span", { class: "label", text: text(label, "name") }))),
      );
    }

    const runsPanel = el("section", {}, el("h2", { text: "Runs" }));
    if (runs.length === 0) {
      runsPanel.append(el("div", { class: "empty" }, el("p", { text: "This card has no run yet." })));
    } else {
      const node = el("table", { class: "table", "data-testid": "cockpit-runs" });
      node.append(
        el("thead", {}, el("tr", {}, el("th", { text: "Run" }), el("th", { text: "Status" }), el("th", {}))),
      );
      const tbody = el("tbody");
      for (const run of runs) {
        const runId = typeof run?.id === "string" ? run.id : "";
        const cell = el("td");
        if (ctx.hasAction("runs.start") && run?.status === "queued") {
          cell.append(
            el("button", {
              class: "button ghost",
              type: "button",
              text: "Start",
              onClick: () =>
                ctx.shell.openAction("runs.start", {
                  title: "Start run",
                  initial: { id: runId },
                  onDone: load,
                }),
            }),
          );
        }
        tbody.append(
          el(
            "tr",
            {},
            el("td", {}, el("a", { href: `/runs/${runId}`, text: short(runId) })),
            el("td", { text: text(run, "status") }),
            cell,
          ),
        );
      }
      node.append(tbody);
      runsPanel.append(node);
    }

    const approvalsPanel = el("section", {}, el("h2", { text: "Approvals" }));
    if (approvals.length === 0) {
      approvalsPanel.append(
        el("div", { class: "empty" }, el("p", { text: "This card has no approval waiting." })),
      );
    } else {
      const node = el("table", { class: "table", "data-testid": "cockpit-approvals" });
      node.append(
        el(
          "thead",
          {},
          el(
            "tr",
            {},
            el("th", { text: "Gate" }),
            el("th", { text: "Status" }),
            el("th", { text: "Target" }),
            el("th", { text: "Policy" }),
            el("th", {}),
          ),
        ),
      );
      const tbody = el("tbody");
      for (const approval of approvals) {
        const gate = text(approval, "gate");
        const [needs, self] = policies.get(gate) ?? ["required", "forbidden"];
        const cell = el("td");
        if (ctx.hasAction("approvals.decide") && approval?.status === "pending") {
          cell.append(
            el("button", {
              class: "button ghost",
              type: "button",
              text: "Decide",
              onClick: () =>
                ctx.shell.openAction("approvals.decide", {
                  title: "Decide approval",
                  initial: { approval_id: approval?.id },
                  onDone: load,
                }),
            }),
          );
        }
        tbody.append(
          el(
            "tr",
            {},
            el("td", { text: gate }),
            el("td", { text: text(approval, "status") }),
            el("td", { text: text(approval, "target") }),
            el("td", { text: `${needs} · self ${self}` }),
            cell,
          ),
        );
      }
      node.append(tbody);
      approvalsPanel.append(node);
    }

    body.append(meta, actions, cardPanel, runsPanel, approvalsPanel);
  }

  await load();
  subscribe(ctx, section, `card:${id},run,approval`, () => void load());
  return { element: section, title: "Card" };
}
