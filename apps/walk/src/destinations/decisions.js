// The Decisions destination (S1): the ladder's recorded decisions and the policies behind them.
// Read-only; a dual-control gate is approved in Approvals, never here.

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

/** @param {string} title @param {string[]} labels @param {any[]} items @param {string} empty @param {string} testId */
function block(title, labels, items, empty, testId) {
  const section = el("section", {}, el("h2", { text: title }));
  if (items.length === 0) {
    section.append(el("div", { class: "empty" }, el("p", { text: empty })));
    return section;
  }
  const table = el("table", { class: "table", "data-testid": testId });
  table.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const row of items) {
    tbody.append(el("tr", {}, ...labels.map((_label, index) => el("td", { text: cells(row)[index] }))));
  }
  table.append(tbody);
  section.append(table);
  return section;
}

/** @param {any} row @returns {string[]} */
function cells(row) {
  return [
    text(row, "kind"),
    text(row, "rung"),
    text(row, "question"),
    text(row, "answer"),
    text(row, "project_id"),
  ];
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  const body = el("div", { class: "destination" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Portfolio & plan" }),
    el("h1", { text: "Decisions" }),
    el("p", { class: "lede", text: "The ladder's recorded decisions and the policies behind them." }),
    body,
  );

  const [decisions, policies] = await Promise.all([
    ctx.hasAction("ladder.list")
      ? ctx.api.invoke("ladder.list", { page: { limit: 50 } }).then((r) => rows(r.output))
      : Promise.resolve([]),
    ctx.hasAction("decision_policies.list")
      ? ctx.api
          .invoke("decision_policies.list", { page: { limit: 50 } })
          .then((r) => rows(r.output))
      : Promise.resolve([]),
  ]);

  body.append(
    el("p", { class: "field-hint" },
      "A dual-control gate is approved in ",
      el("a", { href: "/approvals", text: "Approvals" }),
      "; the missing second approver is shown there, not guessed here.",
    ),
  );
  body.append(
    block(
      "Recorded decisions",
      ["Kind", "Rung", "Question", "Answer", "Project"],
      decisions,
      "No ladder decision in this tenant yet.",
      "decisions-table",
    ),
  );

  const policyBlock = el("section", {}, el("h2", { text: "Decision policies" }));
  if (policies.length === 0) {
    policyBlock.append(
      el("div", { class: "empty" }, el("p", { text: "No decision policy in this tenant yet." })),
    );
  } else {
    const table = el("table", { class: "table", "data-testid": "decisions-policies" });
    table.append(
      el("thead", {}, el("tr", {},
        el("th", { text: "Kind" }),
        el("th", { text: "Key" }),
        el("th", { text: "Status" }),
        el("th", { text: "Version" }),
      )),
    );
    const tbody = el("tbody");
    for (const row of policies) {
      tbody.append(
        el("tr", {},
          el("td", { text: text(row, "kind") }),
          el("td", { text: text(row, "key") }),
          el("td", { text: text(row, "status") }),
          el("td", { text: int(row, "version") }),
        ),
      );
    }
    table.append(tbody);
    policyBlock.append(table);
  }
  body.append(policyBlock);
  return { element: section, title: "Decisions" };
}
