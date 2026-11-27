// The Runs destination (S1): the queue (newest first, with the status filter and the `run` SSE
// topic) and the run detail (the linked card and the gated cancel), as `apps/web` reads them.

import { el } from "../dom.js";
import { streamSse } from "../api/events.js";

const FILTERS = [
  ["all", "All"],
  ["queued", "Queued"],
  ["running", "Running"],
  ["waiting_approval", "Waiting"],
  ["succeeded", "Succeeded"],
  ["failed", "Failed"],
  ["cancelled", "Cancelled"],
  ["expired", "Expired"],
];

const LIVE = new Set(["queued", "running", "waiting_approval"]);

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

/** @param {string} value */
function short(value) {
  return value.length <= 10 ? value : value.slice(0, 10);
}

/**
 * Reconnect the event stream with backoff; abort when the screen leaves the DOM. The shell has
 * no teardown hook, so detachment is observed on the root element (SPEC §5: the caller owns
 * reconnection and resync).
 * @param {any} ctx @param {HTMLElement} root @param {string} topics
 * @param {(frame: {event: string, data: string, id: string}) => void} onFrame
 */
function subscribe(ctx, root, topics, onFrame) {
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
            onFrame(frame);
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
  return ctx.params?.run_id ? detail(ctx, ctx.params.run_id) : queue(ctx);
}

/** @param {any} ctx */
async function queue(ctx) {
  const body = el("div", { class: "destination" });
  const filter = /** @type {HTMLSelectElement} */ (
    el("select", { "aria-label": "Status filter" })
  );
  for (const [value, label] of FILTERS) filter.append(el("option", { value, text: label }));
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Portfolio & plan" }),
    el("h1", { text: "Runs" }),
    el("p", {
      class: "lede",
      text: "The queue, newest first. Every row is the state machine's current position.",
    }),
    el("div", { class: "destination-actions" }, filter),
    body,
  );

  async function load() {
    const chosen = filter.value || "all";
    const input = chosen === "all" ? { limit: 50 } : { status: chosen, limit: 50 };
    const result = ctx.hasAction("runs.list")
      ? await ctx.api.invoke("runs.list", input)
      : { output: [] };
    const items = rows(result.output);
    body.replaceChildren();
    if (items.length === 0) {
      body.append(el("div", { class: "empty" }, el("p", { text: "No runs in this filter." })));
      return;
    }
    const table = el("table", { class: "table", "data-testid": "runs-queue" });
    table.append(
      el(
        "thead",
        {},
        el(
          "tr",
          {},
          el("th", { text: "Run" }),
          el("th", { text: "Card" }),
          el("th", { text: "Attempt" }),
          el("th", { text: "Status" }),
          el("th", { text: "Created" }),
        ),
      ),
    );
    const tbody = el("tbody");
    for (const row of items) {
      const id = text(row, "id");
      const card = text(row, "card_id");
      tbody.append(
        el(
          "tr",
          {},
          el("td", {}, el("a", { href: `/runs/${id}`, text: short(id) })),
          el("td", {}, card === "—" ? "—" : el("a", { href: `/card/${card}`, text: short(card) })),
          el("td", { text: int(row, "attempt") }),
          el("td", { text: text(row, "status") }),
          el("td", { text: text(row, "created_at") }),
        ),
      );
    }
    table.append(tbody);
    body.append(table);
  }

  filter.addEventListener("change", () => void load());
  await load();
  if (ctx.hasAction("runs.list")) {
    subscribe(ctx, section, "run", (frame) => {
      if (frame.event === "resync" || frame.event.startsWith("run.")) void load();
    });
  }
  return { element: section, title: "Runs" };
}

/** @param {any} ctx @param {string} id */
async function detail(ctx, id) {
  const section = el("section", { class: "destination" });
  const body = el("div", { class: "destination" });
  section.append(
    el("p", { class: "eyebrow" }, el("a", { href: "/runs", text: "← All runs" })),
    el("h1", { text: `Run ${short(id)}` }),
    body,
  );

  async function load() {
    const result = ctx.hasAction("runs.get")
      ? await ctx.api.invoke("runs.get", { id })
      : { output: null };
    const run = result.output;
    body.replaceChildren();
    if (!run) {
      body.append(
        el("div", { class: "empty" }, el("p", { text: "No run in the route." })),
      );
      return;
    }
    const card = text(run, "card_id");
    body.append(
      el(
        "dl",
        { class: "facts" },
        el("dt", { text: "Status" }),
        el("dd", { text: text(run, "status") }),
        el("dt", { text: "Attempt" }),
        el("dd", { text: int(run, "attempt") }),
        el("dt", { text: "Version" }),
        el("dd", { text: int(run, "version") }),
        el("dt", { text: "Created" }),
        el("dd", { text: text(run, "created_at") }),
        el("dt", { text: "Card" }),
        el("dd", {}, card === "—" ? "—" : el("a", { href: `/card/${card}`, text: card })),
      ),
    );
    if (text(run, "error_class") !== "—") {
      body.append(el("p", { class: "field-hint", text: `Error class: ${text(run, "error_class")}` }));
    }
    if (ctx.hasAction("runs.cancel") && LIVE.has(text(run, "status"))) {
      body.append(
        el("div", { class: "destination-actions" },
          el("button", {
            class: "button",
            type: "button",
            text: "Cancel run",
            onClick: () =>
              ctx.shell.openAction("runs.cancel", {
                title: "Cancel run",
                initial: { id },
                onDone: load,
              }),
          }),
        ),
      );
    }
  }

  await load();
  return { element: section, title: `Run ${short(id)}` };
}
