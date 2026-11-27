// The Process map destination (S2): `process_maps.*` as the app reads it, the map's steps from
// its `definition`, and the draft/publish/retire writes. The run-position overlay has no read
// yet — the screen names that gap instead of inventing positions.

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

/** @param {any} map @returns {any[]} */
function steps(map) {
  const definition = map?.definition;
  const raw = Array.isArray(definition?.steps) ? definition.steps : Array.isArray(definition) ? definition : [];
  return raw;
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  if (!ctx.hasAction("process_maps.list")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "Process map is planned" }),
        el("p", { class: "field-hint", text: "process_maps.list is not in this role's manifest; the screen appears when the map family is granted." }),
      ),
      title: "Process map is planned",
    };
  }
  return ctx.params?.map_id ? detail(ctx, ctx.params.map_id) : list(ctx);
}

/** @param {any} ctx @param {string} id */
async function detail(ctx, id) {
  const section = el("section", { class: "destination" });
  try {
    const result = await ctx.api.invoke("process_maps.get", { map_id: id });
    const map = result.output ?? {};
    const actions = el("div", { class: "destination-actions" });
    if (ctx.hasAction("process_maps.update") && map.status === "draft") {
      actions.append(
        el("button", {
          class: "button",
          type: "button",
          text: "Edit",
          onClick: () =>
            ctx.shell.openAction("process_maps.update", {
              title: `Edit ${map.title}`,
              initial: { map_id: map.id, title: map.title },
              ifMatch: map.lock_version,
              onDone: () => detail(ctx, id),
            }),
        }),
      );
    }
    if (ctx.hasAction("process_maps.publish") && map.status === "draft") {
      actions.append(
        el("button", { class: "button primary", type: "button", text: "Publish", onClick: () => ctx.shell.openAction("process_maps.publish", { title: "Publish", initial: { map_id: map.id }, onDone: () => detail(ctx, id) }) }),
      );
    }
    const stepRows = steps(map).map((step, index) => [String(index + 1), text(step, "key"), text(step, "title"), text(step, "kind")]);
    section.replaceChildren(
      el("p", { class: "eyebrow" }, el("a", { href: "/build/process-map", text: "← All maps" })),
      el("h1", { text: text(map, "title") }),
      el("p", { class: "lede", text: "The process a card type runs, drawn as its ordered steps." }),
      el("dl", { class: "facts" },
        el("dt", { text: "Key" }), el("dd", { text: text(map, "key") }),
        el("dt", { text: "Status" }), el("dd", { text: text(map, "status") }),
        el("dt", { text: "Version" }), el("dd", { text: num(map, "version") }),
        el("dt", { text: "Project type" }), el("dd", { text: text(map, "project_type_id") }),
      ),
      actions,
      stepRows.length === 0
        ? el("div", { class: "empty" }, el("p", { text: "This map has no steps in its definition yet." }))
        : table("Steps", ["#", "Key", "Title", "Kind"], stepRows, "process-map-steps"),
      el("p", { class: "field-hint", text: "Run positions are not overlaid: no read joins a run to its map nodes yet; the overlay names that gap rather than inventing a position." }),
    );
    return { element: section, title: text(map, "title") };
  } catch (error) {
    section.replaceChildren(el("p", { class: "eyebrow" }, el("a", { href: "/build/process-map", text: "← All maps" })), ctx.shell.problemPanel(error));
    return { element: section, title: "Process map" };
  }
}

/** @param {any} ctx */
async function list(ctx) {
  const body = el("div", { class: "destination-body" });
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading process maps…" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Procedures & authoring" }),
    el("h1", { text: "Process map" }),
    el("p", { class: "lede", text: "Every process map a card type can run, its ordered steps, and its draft/published state." }),
    actions,
    status,
    body,
  );

  async function load() {
    status.textContent = "Loading process maps…";
    actions.replaceChildren();
    body.replaceChildren();
    try {
      const maps = await ctx.api.invoke("process_maps.list", { page: { limit: 100 } }).then((r) => rows(r.output));
      if (ctx.hasAction("process_maps.create")) {
        actions.append(el("button", { class: "button primary", type: "button", text: "New map", onClick: () => ctx.shell.openAction("process_maps.create", { title: "New process map", onDone: load }) }));
      }
      status.textContent = maps.length === 0 ? "" : `${maps.length} map${maps.length === 1 ? "" : "s"}`;
      if (maps.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No process map yet. This is the empty state, not an error." }),
            ctx.hasAction("process_maps.create")
              ? el("button", { class: "button", type: "button", text: "Create the first map", onClick: () => ctx.shell.openAction("process_maps.create", { title: "New process map", onDone: load }) })
              : el("p", { class: "field-hint", text: "Your role cannot create maps." }),
          ),
        );
        return;
      }
      const bodyRows = maps.map((map) => [
        el("a", { href: `/build/process-map/${map.id}`, text: text(map, "title") }),
        text(map, "key"),
        text(map, "status"),
        num(map, "version"),
        String(steps(map).length),
      ]);
      body.append(table("Process maps", ["Title", "Key", "Status", "Version", "Steps"], bodyRows, "process-map-list"));
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Process map" };
}
