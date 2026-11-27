// The Search destination (S3): one ranked `search.query` across the governed assets, cards and
// catalogue entries the tenant holds, and the states the server can return. No second query and
// no client-side re-ranking: the page renders what the action answered.

import { el, clear } from "../dom.js";

/** @param {any} value @returns {any[]} */
function rows(value) {
  if (Array.isArray(value?.items)) return value.items;
  return Array.isArray(value) ? value : [];
}

/** @param {any} row @param {string} key @returns {string} */
function text(row, key) {
  const value = row?.[key];
  if (typeof value === "string" && value !== "") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return "—";
}

/** @param {any} row @param {string[]} keys @returns {string} */
function first(row, keys) {
  for (const key of keys) {
    const shown = text(row, key);
    if (shown !== "—") return shown;
  }
  return "—";
}

/**
 * @param {string[]} labels
 * @param {Array<Array<Node|string>>} bodyRows
 * @param {string} testId
 */
function table(labels, bodyRows, testId) {
  const node = el("table", { class: "table", "data-testid": testId });
  node.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const cells of bodyRows) {
    tbody.append(
      el("tr", {}, ...cells.map((cell) => (cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) })))),
    );
  }
  node.append(tbody);
  return node;
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  if (!ctx.hasAction("search.query")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("p", { class: "eyebrow", text: "Utilities" }),
        el("h1", { text: "Search is planned" }),
        el("p", {
          class: "field-hint",
          text: "search.query is not in this role's manifest; the screen appears when the search scope is granted.",
        }),
      ),
      title: "Search is planned",
    };
  }

  const query = /** @type {HTMLInputElement} */ (
    el("input", {
      type: "search",
      id: "search-query",
      name: "q",
      "aria-label": "Search query",
      autocomplete: "off",
      placeholder: "A title, a key, a phrase",
    })
  );
  const status = el("p", { class: "loading", role: "status" });
  const results = el("div", { class: "destination-body" });
  const form = el("form", { class: "search-form" },
    el("label", { class: "field-label", for: "search-query", text: "Search query" }),
    query,
    el("button", { class: "button primary", type: "submit", text: "Search" }),
  );
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    void run();
  });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Utilities" }),
    el("h1", { text: "Search" }),
    el("p", {
      class: "lede",
      text: "One ranked query across the governed assets, cards and catalogue entries this tenant holds.",
    }),
    form,
    status,
    results,
  );

  async function run() {
    const q = query.value.trim();
    clear(results);
    if (q === "") {
      status.textContent = "Enter a query to search.";
      return;
    }
    status.textContent = "Searching…";
    try {
      const hits = rows((await ctx.api.invoke("search.query", { q })).output);
      status.textContent = hits.length === 0 ? "" : `${hits.length} result${hits.length === 1 ? "" : "s"}`;
      if (hits.length === 0) {
        results.append(el("p", { class: "empty", text: "No result for this query." }));
        return;
      }
      results.append(
        table(
          ["Type", "Title", "Id"],
          hits.map((hit) => [first(hit, ["type", "entity_type"]), text(hit, "title"), text(hit, "id")]),
          "search-results",
        ),
      );
    } catch (error) {
      status.textContent = "";
      results.append(ctx.shell.problemPanel(error));
    }
  }

  return { element: section, title: "Search" };
}
