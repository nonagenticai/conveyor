// The Platform destination (S3): the estate's own ops reads — Flux reconciliation, CI, the
// conveyor namespace's deployments and pods, and the registered host's unit status. One section
// per read: a failing read shows its own problem panel and never blanks the others.

import { el, clear } from "../dom.js";

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
 * The rows a read answered with: the shared page envelope, the CI run list, a bare array, or a
 * single object (a host's status is one row, not a page).
 * @param {any} value @returns {any[]}
 */
function rowsOf(value) {
  if (Array.isArray(value?.items)) return value.items;
  if (Array.isArray(value?.runs)) return value.runs;
  if (Array.isArray(value)) return value;
  if (value && typeof value === "object") return [value];
  return [];
}

/** @param {any} row @returns {string} */
function nameOf(row) {
  const nested = row?.metadata?.name;
  if (typeof nested === "string" && nested !== "") return nested;
  return first(row, ["name", "repo", "host"]);
}

/** @param {any} row @returns {string} */
function namespaceOf(row) {
  const nested = row?.metadata?.namespace;
  if (typeof nested === "string" && nested !== "") return nested;
  return first(row, ["namespace"]);
}

/** @param {any} row @returns {string} */
function stateOf(row) {
  const phase = row?.status?.phase;
  if (typeof phase === "string" && phase !== "") return phase;
  const conditions = row?.status?.conditions;
  if (Array.isArray(conditions) && conditions.length > 0) {
    const last = conditions[conditions.length - 1];
    if (typeof last?.type === "string") {
      return last.status === "True" ? last.type : `${last.type}: ${last.status ?? "Unknown"}`;
    }
  }
  return first(row, ["phase", "state", "conclusion", "status", "ready"]);
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
  if (!ctx.hasAction("ops.flux_status")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("p", { class: "eyebrow", text: "Utilities" }),
        el("h1", { text: "Platform is planned" }),
        el("p", {
          class: "field-hint",
          text: "ops.flux_status is not in this role's manifest; the screen appears when the ops read family is granted.",
        }),
      ),
      title: "Platform is planned",
    };
  }

  const status = el("p", { class: "loading", role: "status", text: "Loading platform status…" });
  const body = el("div", { class: "destination-body" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Utilities" }),
    el("h1", { text: "Platform" }),
    el("p", {
      class: "lede",
      text: "The estate's own reads: Flux reconciliation, recent CI runs, the conveyor namespace's deployments and pods, and the registered host's unit status. A read that fails says so in its own section.",
    }),
    status,
    body,
  );

  /**
   * One read, one section. A failed read renders the shell's problem panel in this section and
   * leaves every other section standing; the table is rendered even with no rows so an empty
   * read is visibly empty rather than missing.
   * @param {string} title
   * @param {string} action
   * @param {string[]} labels
   * @param {string} testId
   * @param {() => Promise<Array<Array<Node|string>>>} read
   */
  async function sectionFor(title, action, labels, testId, read) {
    const block = el("section", {}, el("h2", { text: title }));
    if (!ctx.hasAction(action)) {
      block.append(el("p", { class: "field-hint", text: `${action} is not in this role's manifest.` }));
      return block;
    }
    try {
      const bodyRows = await read();
      block.append(table(labels, bodyRows, testId));
      if (bodyRows.length === 0) {
        block.append(el("p", { class: "field-hint", text: "Nothing in this read. This is the empty state, not an error." }));
      }
    } catch (error) {
      block.append(ctx.shell.problemPanel(error));
    }
    return block;
  }

  /**
   * The host read needs a host and a unit (`ops.host_status` refuses a bare call), so this
   * section is a small form rather than a read that would fail for a missing input.
   * @returns {HTMLElement}
   */
  function hostSection() {
    const block = el("section", {}, el("h2", { text: "Hosts" }));
    if (!ctx.hasAction("ops.host_status")) {
      block.append(el("p", { class: "field-hint", text: "ops.host_status is not in this role's manifest." }));
      return block;
    }
    const hostInput = /** @type {HTMLInputElement} */ (el("input", { type: "text", id: "platform-host", "aria-label": "Host", placeholder: "edge-node-1" }));
    const unitInput = /** @type {HTMLInputElement} */ (el("input", { type: "text", id: "platform-unit", "aria-label": "Unit", placeholder: "k3s" }));
    const result = el("div", {});
    async function readHost() {
      clear(result);
      try {
        const output = (
          await ctx.api.invoke("ops.host_status", { host: hostInput.value.trim(), unit: unitInput.value.trim() })
        ).output;
        result.append(
          table(
            ["Host", "Unit", "State"],
            rowsOf(output).map((row) => [nameOf(row), first(row, ["unit"]), stateOf(row)]),
            "platform-hosts",
          ),
        );
      } catch (error) {
        result.append(ctx.shell.problemPanel(error));
      }
    }
    block.append(
      el(
        "div",
        { class: "field" },
        el("label", { class: "field-label", for: "platform-host", text: "Host" }),
        hostInput,
        el("label", { class: "field-label", for: "platform-unit", text: "Unit" }),
        unitInput,
        el("button", { class: "button", type: "button", text: "Read host", onClick: () => void readHost() }),
      ),
      result,
    );
    return block;
  }

  async function load() {
    clear(body);
    body.append(
      await sectionFor("Flux reconciliation", "ops.flux_status", ["Name", "Kind", "State"], "platform-flux", async () => {
        const output = (await ctx.api.invoke("ops.flux_status", {})).output;
        return rowsOf(output).map((row) => [nameOf(row), first(row, ["kind"]), stateOf(row)]);
      }),
    );
    body.append(
      await sectionFor("CI", "ops.ci_status", ["Name", "Branch", "State"], "platform-ci", async () => {
        const output = (await ctx.api.invoke("ops.ci_status", {})).output;
        return rowsOf(output).map((row) => [nameOf(row), first(row, ["head_branch", "branch"]), stateOf(row)]);
      }),
    );
    body.append(
      await sectionFor(
        "Deployments",
        "ops.deployments",
        ["Name", "Namespace", "State"],
        "platform-deployments",
        async () => {
          const output = (await ctx.api.invoke("ops.deployments", { namespace: "conveyor", limit: 20 })).output;
          return rowsOf(output).map((row) => [nameOf(row), namespaceOf(row), stateOf(row)]);
        },
      ),
    );
    body.append(
      await sectionFor("Pods", "ops.pods", ["Name", "Namespace", "State"], "platform-pods", async () => {
        const output = (await ctx.api.invoke("ops.pods", { namespace: "conveyor", limit: 50 })).output;
        return rowsOf(output).map((row) => [nameOf(row), namespaceOf(row), stateOf(row)]);
      }),
    );
    body.append(hostSection());
    status.textContent = "";
  }

  await load();
  return { element: section, title: "Platform" };
}
