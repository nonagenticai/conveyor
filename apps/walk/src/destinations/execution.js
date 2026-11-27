// The Execution destination (S1): the tenant's runs, the selected run's steps and gates, and the
// tool ledger for it — as `apps/web` reads them. Read-only.

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

/** @param {string} title @param {string[]} labels @param {Array<Array<string>>} body @param {string} testId */
function table(title, labels, body, testId) {
  const block = el("section", {}, el("h2", { text: title }));
  const node = el("table", { class: "table", "data-testid": testId });
  node.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const cells of body) {
    tbody.append(el("tr", {}, ...cells.map((cell) => el("td", { text: cell }))));
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
  const list = el("div", { class: "destination" });
  const detail = el("div", { class: "destination" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Portfolio & plan" }),
    el("h1", { text: "Execution room" }),
    el("p", {
      class: "lede",
      text: "The tenant's runs, with the selected run's steps, gates and tool ledger.",
    }),
    list,
    detail,
  );

  async function loadRun(id) {
    const [run, calls] = await Promise.all([
      ctx.api.invoke("runs.get", { id }).then((r) => r.output),
      ctx.hasAction("tool_calls.list")
        ? ctx.api
            .invoke("tool_calls.list", { run_id: id, limit: 50 })
            .then((r) => rows(r.output))
        : Promise.resolve([]),
    ]);
    detail.replaceChildren();
    detail.append(
      el("p", { class: "eyebrow" }, el("a", { href: `/runs/${id}`, text: "Open the run detail" })),
    );
    const steps = rows(run?.steps);
    detail.append(
      steps.length === 0
        ? el("div", { class: "empty" }, el("p", { text: "No step has run yet." }))
        : table(
            "Steps",
            ["#", "Action", "Attempt", "Status"],
            steps.map((step) => [
              int(step, "ordinal"),
              text(step, "action"),
              int(step, "attempt"),
              text(step, "status"),
            ]),
            "execution-steps",
          ),
    );
    detail.append(
      calls.length === 0
        ? el("div", { class: "empty" }, el("p", { text: "No tool call recorded for this run." }))
        : table(
            "Tools",
            ["Tool", "Version", "Status", "Credential alias", "Evidence"],
            calls.map((call) => [
              text(call, "tool"),
              int(call, "toolkit_version"),
              text(call, "status"),
              text(call, "credential_alias"),
              text(call, "evidence"),
            ]),
            "execution-tool-calls",
          ),
    );
  }

  const result = ctx.hasAction("runs.list")
    ? await ctx.api.invoke("runs.list", { limit: 50 })
    : { output: [] };
  const runs = rows(result.output);
  if (runs.length === 0) {
    list.append(
      el("div", { class: "empty" }, el("p", { text: "No run in this tenant yet; a board card starts one." })),
    );
  } else {
    const node = el("table", { class: "table", "data-testid": "execution-runs" });
    node.append(
      el("thead", {}, el("tr", {},
        el("th", { text: "Run" }),
        el("th", { text: "Card" }),
        el("th", { text: "Status" }),
        el("th", { text: "Attempt" }),
        el("th", { text: "Executor" }),
        el("th", { text: "Binding" }),
        el("th", { text: "Release pin" }),
        el("th", { text: "Started" }),
        el("th", { text: "Finished" }),
        el("th", {}),
      )),
    );
    const tbody = el("tbody");
    for (const run of runs) {
      const id = text(run, "id");
      tbody.append(
        el("tr", {},
          el("td", {}, el("button", {
            class: "button ghost",
            type: "button",
            text: short(id),
            onClick: () => void loadRun(id),
          })),
          el("td", { text: short(text(run, "card_id")) }),
          el("td", { text: text(run, "status") }),
          el("td", { text: int(run, "attempt") }),
          el("td", { text: text(run, "executor_kind") }),
          el("td", { text: short(text(run, "job_binding_id")) }),
          el("td", { text: short(text(run, "release_hash")) }),
          el("td", { text: text(run, "started_at") }),
          el("td", { text: text(run, "finished_at") }),
          el("td", {}, el("a", { href: `/runs/${id}`, text: "Open" })),
        ),
      );
    }
    node.append(tbody);
    list.append(node);
    detail.append(
      el("p", { class: "field-hint", text: "Select a run to read its steps, gates and tools." }),
    );
  }
  return { element: section, title: "Execution room" };
}
