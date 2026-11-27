// The Models destination (S2, Govern & design): `models.list` and `models.get`, each model's
// generated views, and the create/update/validate/publish/import writes. A detail route renders
// one model; an edit carries the row version as If-Match, and every write refetches.

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
        el("h1", { text: "Models is planned" }),
        el("p", { class: "field-hint", text: "models.list is not in this role's manifest; the screen appears when the model family is granted." }),
      ),
      title: "Models is planned",
    };
  }
  return ctx.params?.model_id ? detail(ctx, ctx.params.model_id) : list(ctx);
}

/** @param {any} ctx @param {string} id */
async function detail(ctx, id) {
  const section = el("section", { class: "destination" });
  try {
    const result = await ctx.api.invoke("models.get", { id });
    const model = result.output ?? {};
    const actions = el("div", { class: "destination-actions" });
    const open = (name, label, initial, ifMatch, title = label) =>
      el("button", {
        class: "button",
        type: "button",
        text: label,
        onClick: () => ctx.shell.openAction(name, { title, initial, ifMatch, onDone: () => detail(ctx, id) }),
      });
    if (ctx.hasAction("models.update")) actions.append(open("models.update", "Edit", { id }, model.version, `Edit ${text(model, "title")}`));
    if (ctx.hasAction("models.validate")) actions.append(open("models.validate", "Validate", { id }));
    if (ctx.hasAction("models.publish")) actions.append(open("models.publish", "Publish", { id }));
    if (ctx.hasAction("model_views.generate")) actions.append(open("model_views.generate", "Generate a view", { model_id: id }));
    const views = ctx.hasAction("model_views.list")
      ? await ctx.api.invoke("model_views.list", { model_id: id, page: { limit: 50 } }).then((r) => rows(r.output)).catch(() => [])
      : [];
    section.append(
      el("p", { class: "eyebrow" }, el("a", { href: "/design/models", text: "← All models" })),
      el("h1", { text: text(model, "title") }),
      el("p", { class: "lede", text: text(model, "description") }),
      el("dl", { class: "facts" },
        el("dt", { text: "Language" }), el("dd", { text: `${text(model, "language")} ${text(model, "language_version")}` }),
        el("dt", { text: "Head revision" }), el("dd", { text: num(model, "head_revision") }),
        el("dt", { text: "Published revision" }), el("dd", { text: text(model, "published_revision_id") }),
        el("dt", { text: "Version" }), el("dd", { text: num(model, "version") }),
        el("dt", { text: "Updated" }), el("dd", { text: text(model, "updated_at") }),
      ),
      actions,
    );
    if (views.length > 0) {
      section.append(table("Views", ["Name", "Kind", "Status"], views.map((view) => [text(view, "name"), text(view, "kind"), text(view, "status")]), "models-views"));
    }
    return { element: section, title: text(model, "title") };
  } catch (error) {
    section.replaceChildren(el("p", { class: "eyebrow" }, el("a", { href: "/design/models", text: "← All models" })), ctx.shell.problemPanel(error));
    return { element: section, title: "Model" };
  }
}

/** @param {any} ctx */
async function list(ctx) {
  const body = el("div", { class: "destination-body" });
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading models…" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Govern & design" }),
    el("h1", { text: "Models" }),
    el("p", { class: "lede", text: "The architecture models, their languages and revisions, the views generated from them, and the import and validate acts." }),
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
      if (ctx.hasAction("models.create")) actions.append(el("button", { class: "button primary", type: "button", text: "New model", onClick: () => ctx.shell.openAction("models.create", { title: "New model", onDone: load }) }));
      if (ctx.hasAction("models.import_xmi")) actions.append(el("button", { class: "button", type: "button", text: "Import XMI", onClick: () => ctx.shell.openAction("models.import_xmi", { title: "Import XMI", onDone: load }) }));
      status.textContent = models.length === 0 ? "" : `${models.length} model${models.length === 1 ? "" : "s"}`;
      if (models.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No model yet. This is the empty state, not an error." }),
            ctx.hasAction("models.create")
              ? el("button", { class: "button", type: "button", text: "Create the first model", onClick: () => ctx.shell.openAction("models.create", { title: "New model", onDone: load }) })
              : el("p", { class: "field-hint", text: "Your role cannot create models." }),
          ),
        );
        return;
      }
      const bodyRows = models.map((model) => [
        el("a", { href: `/design/models/${model.id}`, text: text(model, "title") }),
        `${text(model, "language")} ${text(model, "language_version")}`,
        num(model, "head_revision"),
        text(model, "published_revision_id"),
        num(model, "version"),
      ]);
      body.append(table("Models", ["Title", "Language", "Head revision", "Published revision", "Version"], bodyRows, "models-list"));
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Models" };
}
