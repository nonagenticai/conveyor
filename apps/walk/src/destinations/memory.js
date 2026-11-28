// The Memory destination (#888): the app's read is `memory.recall` (the card's hot rows and its
// semantic digest, apps/web/src/screens/memory.rs); the lesson family is propose → review →
// promote with no list read. A recall with no hot and no cold row is the empty state; a failed
// read is the server's problem panel.

import { el, clear } from "../dom.js";

/** @param {any} value @returns {any[]} */
function rows(value) {
  if (Array.isArray(value?.items)) return value.items;
  return Array.isArray(value) ? value : [];
}

/** @param {any} row @param {...string} keys @returns {string} */
function text(row, ...keys) {
  for (const key of keys) {
    const value = row?.[key];
    if (typeof value === "string" && value !== "") return value;
    if (typeof value === "number" || typeof value === "boolean") return String(value);
  }
  return "—";
}

/**
 * @param {string} title
 * @param {string[]} labels
 * @param {Array<Array<Node|string>>} bodyRows
 * @param {string} testId
 */
function table(title, labels, bodyRows, testId) {
  const block = el("section", {}, el("h2", { text: title }));
  const node = el("table", { class: "table", "data-testid": testId });
  node.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const cells of bodyRows) {
    tbody.append(
      el(
        "tr",
        {},
        ...cells.map((cell) =>
          cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) }),
        ),
      ),
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
  if (!ctx.hasAction("memory.recall")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("p", { class: "eyebrow", text: "Knowledge & memory" }),
        el("h1", { text: "Memory is planned" }),
        el("p", {
          class: "field-hint",
          text: "memory.recall is not in this role's manifest; the screen appears when the memory read is granted.",
        }),
      ),
      title: "Memory is planned",
    };
  }

  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status" });
  const results = el("div", { class: "destination-body" });
  const cardInput = /** @type {HTMLInputElement} */ (
    el("input", { type: "text", placeholder: "card id (uuid)", "aria-label": "Work package" })
  );
  cardInput.id = "memory-card";
  const queryInput = /** @type {HTMLInputElement} */ (
    el("input", { type: "text", placeholder: "optional digest query", "aria-label": "Memory query" })
  );
  queryInput.id = "memory-query";

  /** @param {string} name @param {string} label */
  function button(name, label) {
    return el("button", {
      class: "button",
      type: "button",
      text: label,
      onClick: () => ctx.shell.openAction(name, { title: label }),
    });
  }

  async function recall() {
    const cardId = cardInput.value.trim();
    const question = queryInput.value.trim();
    clear(results);
    if (cardId === "") {
      status.textContent = "Name a work package (its card id).";
      return;
    }
    status.textContent = "Recalling…";
    try {
      const input = { card_id: cardId, limit: 50 };
      if (question !== "") input.query = question;
      const result = await ctx.api.invoke("memory.recall", input);
      status.textContent = "";
      const hot = rows(result.output?.hot);
      const cold = rows(result.output?.cold);
      if (hot.length === 0 && cold.length === 0) {
        results.append(el("p", { class: "empty", text: "No memory row for this work package yet." }));
        return;
      }
      if (hot.length > 0) {
        results.append(
          table(
            "Hot rows",
            ["Kind", "Source", "Body"],
            hot.map((row) => [text(row, "kind"), text(row, "source"), text(row, "body")]),
            "memory-hot",
          ),
        );
      }
      if (cold.length > 0) {
        results.append(
          table(
            "Digest hits",
            ["Source", "Ref", "Text"],
            cold.map((hit) => [text(hit, "source", "path"), text(hit, "ref", "id"), text(hit, "text", "content", "snippet")]),
            "memory-cold",
          ),
        );
      }
      const warnings = Array.isArray(result.output?.warnings)
        ? result.output.warnings.filter((warning) => typeof warning === "string")
        : [];
      if (result.output?.degraded) {
        results.append(
          el("p", {
            class: "field-hint",
            text: warnings.length > 0 ? `Degraded: ${warnings.join(" · ")}` : "Degraded: the digest could not be searched.",
          }),
        );
      }
    } catch (error) {
      status.textContent = "";
      clear(results);
      results.append(ctx.shell.problemPanel(error));
    }
  }

  for (const [name, label] of [
    ["lessons.propose", "Propose a lesson"],
    ["lessons.review", "Review a lesson"],
    ["lessons.promote", "Promote a lesson"],
  ]) {
    if (ctx.hasAction(name)) actions.append(button(name, label));
  }

  const form = el(
    "div",
    { class: "field" },
    el("label", { class: "field-label", for: "memory-card", text: "Work package (card id)" }),
    cardInput,
    el("label", { class: "field-label", for: "memory-query", text: "Digest query (optional)" }),
    queryInput,
    el("button", { class: "button primary", type: "button", text: "Recall", onClick: () => void recall() }),
  );

  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Knowledge & memory" }),
    el("h1", { text: "Memory" }),
    el("p", {
      class: "lede",
      text: "The hot rows a work package learned, its derived digest, and the lesson lifecycle that can promote a finding to doctrine.",
    }),
    actions,
    form,
    status,
    results,
  );

  return { element: section, title: "Memory" };
}
