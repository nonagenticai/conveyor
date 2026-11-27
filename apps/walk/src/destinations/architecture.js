// The System architecture destination (S2, Govern & design): `models.list` as the app reads it,
// each model's generated views, and the view-generate write. A model's own detail lives under
// Requirements & models; this screen is the architecture-level reading of the same repository.

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
function num(row, key) {
  const value = row?.[key];
  return typeof value === "number" ? String(value) : "—";
}

/** @param {string} title @param {string[]} labels @param {Array<Array<Node|string>>} bodyRows @param {string} testId */
function table(title, labels, bodyRows, testId) {
  const block = el("section", {}, el("h2", { text: title }));
  const node = el("table", { class: "table", "data-testid": testId });
  node.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const cells of bodyRows) {
    tbody.append(el("tr", {}, ...cells.map((cell) => (cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) })))));
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
  if (!ctx.hasAction("models.list")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "Architecture is planned" }),
        el("p", { class: "field-hint", text: "models.list is not in this role's manifest; the screen appears when the model family is granted." }),
      ),
      title: "Architecture is planned",
    };
  }

  const body = el("div", { class: "destination-body" });
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading models…" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Govern & design" }),
    el("h1", { text: "System architecture" }),
    el("p", {
      class: "lede",
      text: "The models that describe the tenant's system, their languages and head revisions, the views generated from each, and the generate act that refreshes a view.",
    }),
    actions,
    status,
    body,
  );

  async function load() {
    status.textContent = "Loading models…";
    actions.replaceChildren();
    body.replaceChildren();
    try {
      const models = await ctx.api.invoke("models.list", { page: { limit: 100 } }).then((r) => rows(r.output));
      if (ctx.hasAction("model_views.generate")) actions.append(el("button", { class: "button", type: "button", text: "Generate a view", onClick: () => ctx.shell.openAction("model_views.generate", { title: "Generate a view", onDone: load }) }));
      status.textContent = models.length === 0 ? "" : `${models.length} model${models.length === 1 ? "" : "s"}`;
      if (models.length === 0) {
        body.append(el("div", { class: "empty" }, el("p", { text: "No model yet. This is the empty state, not an error." })));
        return;
      }
      const modelRows = models.map((model) => [text(model, "title"), text(model, "language"), text(model, "language_version"), num(model, "head_revision"), num(model, "version")]);
      body.append(table("Models", ["Title", "Language", "Version", "Head revision", "Row version"], modelRows, "architecture-models"));
      if (ctx.hasAction("model_views.list") && models.length > 0) {
        const views = await ctx.api.invoke("model_views.list", { model_id: models[0].id, page: { limit: 50 } }).then((r) => rows(r.output)).catch(() => []);
        if (views.length > 0) {
          body.append(table("Views of the head model", ["Name", "Kind", "Status"], views.map((view) => [text(view, "name"), text(view, "kind"), text(view, "status")]), "architecture-views"));
        }
      }
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "System architecture" };
}
