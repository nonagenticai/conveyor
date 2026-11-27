// The Approvals destination (S3): `approvals.list` and `approval_policies.list` as the app reads
// them, the principal's own decide and request sheets, and the states the server can return.
// The walk never decides for the principal: a decision is only ever what the person submits.

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
      el("tr", {}, ...cells.map((cell) => (cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) })))),
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
  if (!ctx.hasAction("approvals.list")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("p", { class: "eyebrow", text: "Utilities" }),
        el("h1", { text: "Approvals is planned" }),
        el("p", {
          class: "field-hint",
          text: "approvals.list is not in this role's manifest; the screen appears when the approval family is granted.",
        }),
      ),
      title: "Approvals is planned",
    };
  }

  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading approvals…" });
  const body = el("div", { class: "destination-body" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Utilities" }),
    el("h1", { text: "Approvals" }),
    el("p", {
      class: "lede",
      text: "The gates waiting for a person, the policy that governs each action, and the principal's own decision. The walk never acquires an approval on your behalf.",
    }),
    actions,
    status,
    body,
  );

  /**
   * @param {any} approval
   * @param {"approve"|"reject"} decision
   * @param {string} label
   */
  function decideButton(approval, decision, label) {
    return el("button", {
      class: "button ghost",
      type: "button",
      text: label,
      onClick: () =>
        ctx.shell.openAction("approvals.decide", {
          title: label,
          initial: { approval_id: approval.id, decision },
          onDone: load,
        }),
    });
  }

  async function load() {
    status.textContent = "Loading approvals…";
    clear(actions);
    clear(body);
    if (ctx.hasAction("approvals.request")) {
      actions.append(
        el("button", {
          class: "button primary",
          type: "button",
          text: "Request an approval",
          onClick: () =>
            ctx.shell.openAction("approvals.request", {
              title: "Request an approval",
              onDone: load,
            }),
        }),
      );
    }
    try {
      const approvals = rows((await ctx.api.invoke("approvals.list", { limit: 100 })).output);
      const policies = ctx.hasAction("approval_policies.list")
        ? rows((await ctx.api.invoke("approval_policies.list", {})).output)
        : [];
      status.textContent = approvals.length === 0 ? "" : `${approvals.length} approval${approvals.length === 1 ? "" : "s"}`;
      if (approvals.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No approval yet. This is the empty state, not an error." }),
          ),
        );
      } else {
        body.append(
          table(
            "Gates",
            ["Gate", "Kind", "Status", "Decisions", "Requested by", "Actions"],
            approvals.map((approval) => [
              text(approval, "gate"),
              text(approval, "kind"),
              text(approval, "status"),
              text(approval, "required_decisions"),
              text(approval, "requested_by_label"),
              ctx.hasAction("approvals.decide") && approval.status === "pending"
                ? el("div", {},
                    decideButton(approval, "approve", "Approve"),
                    decideButton(approval, "reject", "Reject"),
                  )
                : "—",
            ]),
            "approvals-list",
          ),
        );
      }
      if (policies.length > 0) {
        body.append(
          table(
            "Approval policies",
            ["Name", "Kind", "Status"],
            policies.map((policy) => [text(policy, "name"), text(policy, "kind"), text(policy, "status")]),
            "approvals-policies",
          ),
        );
      }
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Approvals" };
}
