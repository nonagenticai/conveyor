// The Improve destination (S3): the kata's Gets, the lookup that stands in for the absent
// `kata.list`, and the lesson the organisation carries forward. Every write opens the
// schema-rendered action sheet and refetches the kata; a read failure is the server's own
// problem panel. No row is fabricated: without a looked-up kata, the Gets act on the id the
// operator names.

import { el, clear } from "../dom.js";

/** @param {any} value @returns {any[]} */
function rows(value) {
  if (Array.isArray(value?.items)) return value.items;
  return Array.isArray(value) ? value : [];
}

/** @param {any} row @param {...string} keys */
function text(row, ...keys) {
  for (const key of keys) {
    const value = row?.[key];
    if (typeof value === "string" && value !== "") return value;
    if (typeof value === "number") return String(value);
  }
  return "—";
}

/** @param {any} row @param {string} key */
function num(row, key) {
  const value = row?.[key];
  return typeof value === "number" ? String(value) : "—";
}

/** The Gets' `content` is the recorded evidence, a JSON value: shown, never interpreted.
 * @param {any} value */
function json(value) {
  if (value === undefined || value === null) return "—";
  if (typeof value === "string") return value === "" ? "—" : value;
  try {
    const encoded = JSON.stringify(value);
    return encoded === undefined ? "—" : encoded;
  } catch {
    return "—";
  }
}

/** @param {string} title @param {string[]} labels @param {Array<Array<Node|string>>} bodyRows @param {string} testId */
function table(title, labels, bodyRows, testId) {
  const block = el("section", {}, el("h2", { text: title }));
  const node = el("table", { class: "table", "data-testid": testId });
  node.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const cells of bodyRows) {
    tbody.append(
      el("tr", {}, ...cells.map((cell) => (cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) })))),
    );
  }
  node.append(tbody);
  block.append(node);
  return block;
}

/** The kata's Gets, in the procedure's order. Each one is evidence written by its own action.
 * @type {Array<[string, string]>} */
const ACTS = [
  ["Observe", "kata.observe"],
  ["Analyse", "kata.analyse"],
  ["Hypothesise", "kata.hypothesise"],
  ["Propose", "kata.propose"],
  ["Measure", "kata.measure"],
  ["Standardise", "kata.standardise"],
  ["Sustain", "kata.sustain"],
  ["Stretch", "kata.stretch"],
  ["Adopt", "kata.adopt"],
];

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  if (!ctx.hasAction("kata.open")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "Improve is planned" }),
        el("p", {
          class: "field-hint",
          text: "kata.open is not in this role's manifest; the screen appears when the kata family is granted.",
        }),
      ),
      title: "Improve is planned",
    };
  }

  /** The improvement `kata.get` last returned, and the id currently named. */
  let improvement = null;
  const idInput = /** @type {HTMLInputElement} */ (
    el("input", { type: "text", placeholder: "the improvement (kata) id", "aria-label": "Improvement id" })
  );
  idInput.id = "improve-id";
  const status = el("p", { class: "loading", role: "status" });
  const resultHost = el("div", { class: "destination-body" });
  const actions = el("div", { class: "destination-actions" });
  const acts = el("div", { class: "destination-actions" });

  /** The id an act should pre-fill: the fetched improvement, else what the operator named. */
  function actInitial() {
    const id =
      improvement && typeof improvement.id === "string" && improvement.id !== ""
        ? improvement.id
        : idInput.value.trim();
    return id === "" ? {} : { id };
  }

  /** @param {string} name @param {string} label @param {Record<string, unknown>} [initial] */
  function button(name, label, initial = {}) {
    return el("button", {
      class: "button",
      type: "button",
      text: label,
      onClick: () => ctx.shell.openAction(name, { title: label, initial, onDone: load }),
    });
  }

  /** An act takes the looked-up improvement as its id, resolved when the sheet opens. */
  function act(name, label) {
    return el("button", {
      class: "button",
      type: "button",
      text: label,
      onClick: () =>
        ctx.shell.openAction(name, { title: label, initial: actInitial(), onDone: load }),
    });
  }

  function renderImprovement() {
    clear(resultHost);
    if (!improvement || typeof improvement !== "object") {
      resultHost.append(
        el("p", {
          class: "empty",
          text: "No improvement read in this session. There is no kata.list read; name the id the open response or the board carries.",
        }),
      );
      return;
    }
    resultHost.append(
      el(
        "dl",
        { class: "facts" },
        el("dt", { text: "Improvement" }), el("dd", { text: text(improvement, "id") }),
        el("dt", { text: "Title" }), el("dd", { text: text(improvement, "title") }),
        el("dt", { text: "Status" }), el("dd", { text: text(improvement, "status") }),
        el("dt", { text: "Signature" }), el("dd", { text: text(improvement, "signature") }),
      ),
    );
    const gets = Array.isArray(improvement.gets) ? improvement.gets : [improvement];
    resultHost.append(
      table(
        "Gets",
        ["Get", "Content", "Recorded by", "Recorded at"],
        gets.map((get) => [text(get, "get"), json(get.content), text(get, "recorded_by"), text(get, "recorded_at")]),
        "improve-kata",
      ),
    );
  }

  async function load() {
    improvement = null;
    const id = idInput.value.trim();
    if (id === "") {
      status.textContent = "";
      renderImprovement();
      return;
    }
    status.textContent = "Loading the improvement…";
    try {
      const result = await ctx.api.invoke("kata.get", { id });
      status.textContent = "";
      improvement = result.output ?? null;
      renderImprovement();
    } catch (error) {
      status.textContent = "";
      clear(resultHost);
      resultHost.append(ctx.shell.problemPanel(error));
    }
  }

  actions.append(button("kata.open", "Open an improvement"));
  if (ctx.hasAction("lessons.propose")) actions.append(button("lessons.propose", "Propose a lesson"));
  for (const [label, name] of ACTS) {
    if (ctx.hasAction(name)) acts.append(act(name, label));
  }

  const lookup = el(
    "div",
    { class: "field" },
    el("label", { class: "field-label", for: "improve-id", text: "Improvement id" }),
    idInput,
    el("button", { class: "button", type: "button", text: "Look up", onClick: () => void load() }),
  );

  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Measure & improve" }),
    el("h1", { text: "Improve" }),
    el("p", {
      class: "lede",
      text: "The kata that measures a systemic finding: its Gets, the held-out measurement and a person's adoption. There is no kata.list read, so an improvement is named by its id.",
    }),
    actions,
    lookup,
    acts,
    status,
    resultHost,
  );

  await load();
  return { element: section, title: "Improve" };
}
