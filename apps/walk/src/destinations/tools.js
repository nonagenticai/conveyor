// The Tools destination (S1): the published catalogue, the grants (bindings) that narrow a
// toolkit to a job binding or run, the call ledger, and the intake writes — as `apps/web`
// reads them. A grant's update and retire carry the row's version as If-Match.

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

/** @param {string} value */
function short(value) {
  return value === "—" || value.length <= 8 ? value : value.slice(0, 8);
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
  if (!ctx.hasAction("tools.list")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "Tools is planned" }),
        el("p", {
          class: "field-hint",
          text: "tools.list is not in this role's manifest; the screen appears when T13 lands.",
        }),
      ),
      title: "Tools is planned",
    };
  }

  const body = el("div", { class: "destination" });
  const actions = el("div", { class: "destination-actions" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Organisation" }),
    el("h1", { text: "Tools" }),
    el("p", {
      class: "lede",
      text: "The published catalogue, its versions and probes, the grants that bind a toolkit to a job or run, the call ledger and the path that brings a new tool in.",
    }),
    actions,
    body,
  );

  /** @param {string} name @param {string} label @param {Record<string, unknown>} input */
  function button(name, label, input = {}) {
    return el("button", {
      class: "button",
      type: "button",
      text: label,
      onClick: () => ctx.shell.openAction(name, { title: label, initial: input, onDone: load }),
    });
  }

  async function load() {
    const [tools, toolkits, bindings, calls] = await Promise.all([
      ctx.api.invoke("tools.list", { page: { limit: 50 } }).then((r) => rows(r.output)),
      ctx.hasAction("toolkits.list")
        ? ctx.api.invoke("toolkits.list", { page: { limit: 50 } }).then((r) => rows(r.output))
        : Promise.resolve([]),
      ctx.hasAction("tool_bindings.list")
        ? ctx.api.invoke("tool_bindings.list", { page: { limit: 50 } }).then((r) => rows(r.output))
        : Promise.resolve([]),
      ctx.hasAction("tool_calls.list")
        ? ctx.api.invoke("tool_calls.list", { page: { limit: 200 } }).then((r) => rows(r.output))
        : Promise.resolve([]),
    ]);

    actions.replaceChildren();
    if (ctx.hasAction("toolkits.publish")) {
      actions.append(button("toolkits.publish", "Publish toolkit"));
    }
    if (ctx.hasAction("toolkits.register_mcp")) {
      actions.append(button("toolkits.register_mcp", "Register MCP toolkit"));
    }
    if (ctx.hasAction("toolkits.import_openapi")) {
      actions.append(button("toolkits.import_openapi", "Import OpenAPI"));
    }

    body.replaceChildren();

    if (tools.length === 0) {
      body.append(
        el("div", { class: "empty" }, el("p", { text: "No published tool in this tenant; toolkits.publish imports the embedded manifests and the Bring your own tab registers a new source." })),
      );
    } else {
      body.append(
        table(
          "Catalogue",
          ["Toolkit", "Version", "Tool", "Effect", "Idempotency", "Status", "Summary"],
          tools.map((row) => [
            text(row, "toolkit"),
            int(row, "toolkit_version"),
            text(row, "tool"),
            text(row, "effect"),
            text(row, "idempotency"),
            text(row, "status"),
            text(row, "summary"),
          ]),
          "tools-catalogue",
        ),
      );
    }

    const grants = el("section", {}, el("h2", { text: "Grants" }));
    if (bindings.length === 0) {
      grants.append(
        el("div", { class: "empty" }, el("p", { text: "No grant in this tenant; a binding names exactly one job binding or run and one published toolkit version." })),
      );
    } else {
      const node = el("table", { class: "table", "data-testid": "tools-bindings" });
      node.append(
        el("thead", {}, el("tr", {},
          el("th", { text: "Grant" }),
          el("th", { text: "Job binding" }),
          el("th", { text: "Run" }),
          el("th", { text: "Toolkit" }),
          el("th", { text: "Ceiling" }),
          el("th", { text: "Credential" }),
          el("th", { text: "Enabled" }),
          el("th", { text: "Version" }),
          el("th", {}),
        )),
      );
      const tbody = el("tbody");
      for (const row of bindings) {
        const id = text(row, "id");
        const version = row?.version;
        const enabled = row?.enabled === true;
        const cells = [
          el("td", { text: short(id) }),
          el("td", { text: short(text(row, "job_binding_id")) }),
          el("td", { text: short(text(row, "run_id")) }),
          el("td", { text: short(text(row, "toolkit_id")) }),
          el("td", { text: text(row, "effect_ceiling") }),
          el("td", { text: text(row, "credential_alias") }),
          el("td", { text: enabled ? "active" : "disabled" }),
          el("td", { text: int(row, "version") }),
          el("td", {}),
        ];
        if (ctx.hasAction("tool_bindings.update")) {
          cells[8].append(
            el("button", {
              class: "button ghost",
              type: "button",
              text: "Edit",
              onClick: () =>
                ctx.shell.openAction("tool_bindings.update", {
                  title: "Edit grant",
                  initial: {
                    id,
                    effect_ceiling: row?.effect_ceiling,
                    credential_alias: row?.credential_alias,
                  },
                  ifMatch: version,
                  onDone: load,
                }),
            }),
          );
        }
        if (ctx.hasAction("tool_bindings.retire")) {
          cells[8].append(
            el("button", {
              class: "button ghost",
              type: "button",
              text: "Retire",
              onClick: () =>
                ctx.shell.openAction("tool_bindings.retire", {
                  title: "Retire grant",
                  initial: { id },
                  ifMatch: version,
                  onDone: load,
                }),
            }),
          );
        }
        tbody.append(el("tr", {}, ...cells));
      }
      node.append(tbody);
      grants.append(node);
    }
    body.append(grants);

    body.append(
      toolkits.length === 0
        ? el("div", { class: "empty" }, el("p", { text: "No toolkit is registered in this tenant yet." }))
        : table(
            "Toolkits",
            ["Toolkit", "Version", "Status"],
            toolkits.map((row) => [text(row, "toolkit"), int(row, "version"), text(row, "status")]),
            "tools-toolkits",
          ),
    );

    body.append(
      calls.length === 0
        ? el("div", { class: "empty" }, el("p", { text: "No tool call in this tenant's ledger; the broker writes one row per call after the destination answers." }))
        : table(
            "Call ledger",
            ["Call", "Run", "Tool", "Toolkit", "Version", "Status", "HTTP", "Latency"],
            calls.map((row) => [
              short(text(row, "id")),
              short(text(row, "run_id")),
              text(row, "tool"),
              text(row, "toolkit"),
              int(row, "toolkit_version"),
              text(row, "status"),
              int(row, "http_status"),
              `${int(row, "latency_ms")} ms`,
            ]),
            "tools-ledger",
          ),
    );
  }

  await load();
  return { element: section, title: "Tools" };
}
