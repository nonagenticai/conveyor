// The Organisation destination (S1): the eight reads and the three writes `apps/web` renders
// (T18-2). An absent family renders the same empty message as the app; a write opens the
// schema sheet and the server's refusal stays verbatim (SPEC §4–§6).

import { el } from "../dom.js";

/**
 * The rows of an action output: `{items: [...]}` (a page) or a bare array.
 * @param {any} value
 * @returns {any[]}
 */
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

/** @param {any} row @param {string} key */
function count(row, key) {
  return Array.isArray(row?.[key]) ? String(row[key].length) : "0";
}

/**
 * @typedef {{key: string, title: string, action: string, input: unknown,
 *            empty: string, columns?: Array<{label: string, cell: (row: any) => string}>}} Section
 */

/** @type {Section[]} */
const SECTIONS = [
  {
    key: "industries",
    title: "Industries",
    action: "industries.list",
    input: {},
    empty: "No industries in this tenant yet.",
    columns: [
      { label: "Key", cell: (row) => text(row, "key") },
      { label: "Name", cell: (row) => text(row, "name") },
      { label: "Status", cell: (row) => text(row, "status") },
    ],
  },
  {
    key: "departments",
    title: "Departments",
    action: "departments.list",
    input: {},
    empty: "No departments in this tenant yet.",
    columns: [
      { label: "Name", cell: (row) => text(row, "name") },
      { label: "Parent", cell: (row) => text(row, "parent_id") },
      { label: "Industry", cell: (row) => text(row, "industry_id") },
      {
        label: "Primary",
        cell: (row) => (row?.is_primary === true ? "yes" : "no"),
      },
      { label: "Version", cell: (row) => int(row, "version") },
    ],
  },
  {
    key: "jobs",
    title: "Catalogue positions",
    action: "job_catalogue.list",
    input: {},
    empty: "No catalogue positions in this tenant yet.",
    columns: [
      { label: "Key", cell: (row) => text(row, "key") },
      { label: "Title", cell: (row) => text(row, "title") },
      { label: "Family", cell: (row) => text(row, "family") },
      { label: "Status", cell: (row) => text(row, "status") },
      { label: "Departments", cell: (row) => count(row, "departments") },
    ],
  },
  {
    key: "approval-policies",
    title: "Approval policies",
    action: "approval_policies.list",
    input: {},
    empty: "No approval policies in this tenant yet.",
    columns: [
      { label: "Action", cell: (row) => text(row, "action") },
      { label: "Approval", cell: (row) => text(row, "approval") },
      { label: "Self approval", cell: (row) => text(row, "self_approval") },
    ],
  },
  {
    key: "decision-policies",
    title: "Decision policies",
    action: "decision_policies.list",
    input: { page: { limit: 50 } },
    empty: "No decision policies in this tenant yet.",
    columns: [
      { label: "Kind", cell: (row) => text(row, "kind") },
      { label: "Gate", cell: (row) => text(row, "gate") },
      { label: "Threshold", cell: (row) => int(row, "threshold") },
      { label: "Fail policy", cell: (row) => text(row, "fail_policy") },
      { label: "Action", cell: (row) => text(row, "action") },
      { label: "Version", cell: (row) => int(row, "version") },
    ],
  },
  {
    key: "refinements",
    title: "Refinements",
    action: "refinements.list",
    input: { page: { limit: 50 } },
    empty: "No refinements in this tenant yet.",
    columns: [
      { label: "Policy", cell: (row) => text(row, "policy_id") },
      { label: "Threshold override", cell: (row) => int(row, "threshold_override") },
      { label: "Fail action", cell: (row) => text(row, "fail_action") },
      { label: "Laya config", cell: (row) => text(row, "laya_config_id") },
      { label: "Version", cell: (row) => int(row, "version") },
    ],
  },
  {
    key: "ladder",
    title: "Decision ladder",
    action: "ladder.list",
    input: { page: { limit: 50 } },
    empty: "No ladder decisions in this tenant yet.",
    columns: [
      { label: "Kind", cell: (row) => text(row, "kind") },
      { label: "Rung", cell: (row) => text(row, "rung") },
      { label: "Question", cell: (row) => text(row, "question") },
      { label: "Answer", cell: (row) => text(row, "answer") },
    ],
  },
];

/** @param {Section} section @param {any[]} items */
function block(section, items) {
  const wrap = el("section", {}, el("h2", { text: section.title }));
  if (items.length === 0) {
    wrap.append(el("div", { class: "empty" }, el("p", { text: section.empty })));
    return wrap;
  }
  const columns = section.columns ?? [];
  wrap.append(
    el(
      "table",
      { class: "table", "data-testid": `organisation-${section.key}` },
      el("thead", {}, el("tr", {}, ...columns.map((column) => el("th", { text: column.label })))),
      el(
        "tbody",
        {},
        ...items.map((row) =>
          el(
            "tr",
            {},
            ...columns.map((column) => el("td", { text: column.cell(row) })),
          ),
        ),
      ),
    ),
  );
  return wrap;
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  const actions = el("div", { class: "destination-actions" });
  const body = el("div", { class: "destination" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Organisation" }),
    el("h1", { text: "Organisation" }),
    el(
      "p",
      { class: "lede", text: "The tenant's industries, departments, positions, policies and ladder." },
    ),
    actions,
    body,
  );

  /** @param {string} name @param {string} label */
  function writeButton(name, label) {
    return el("button", {
      class: "button",
      type: "button",
      text: label,
      onClick: () => ctx.shell.openAction(name, { title: label, onDone: load }),
    });
  }

  function renderActions() {
    actions.replaceChildren();
    if (ctx.hasAction("departments.create")) {
      actions.append(writeButton("departments.create", "Create department"));
    }
    if (ctx.hasAction("job_catalogue.create")) {
      actions.append(writeButton("job_catalogue.create", "Create position"));
    }
    if (ctx.hasAction("approval_policies.set")) {
      actions.append(writeButton("approval_policies.set", "Set approval policy"));
    }
  }

  async function load() {
    const results = await Promise.all(
      SECTIONS.map((item) =>
        ctx.hasAction(item.action)
          ? ctx.api.invoke(item.action, item.input).then((result) => rows(result.output))
          : Promise.resolve([]),
      ),
    );
    body.replaceChildren();
    SECTIONS.forEach((item, index) => body.append(block(item, results[index])));
    const laya = ctx.hasAction("laya_configs.list")
      ? rows((await ctx.api.invoke("laya_configs.list", {})).output).length
      : 0;
    body.append(
      el("p", {
        class: "field-hint",
        text: `${laya} laya configuration row(s) readable for this principal.`,
      }),
    );
  }

  await load();
  renderActions();
  return { element: section, title: "Organisation" };
}
