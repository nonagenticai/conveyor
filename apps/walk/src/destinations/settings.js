// The Settings destination (S3): the four tabs — admin, inference, members and roles — as the
// app reads them, each write behind its action sheet, and every state the server can return.
// The tab is the pathname's last segment; `/settings` alone lands on admin.

import { el, clear } from "../dom.js";

const TABS = ["admin", "inference", "members", "roles"];

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

/** The tab the pathname names; anything unknown falls back to admin. @returns {string} */
function currentTab() {
  const parts = window.location.pathname.split("/").filter((part) => part !== "");
  const last = parts.length > 0 ? parts[parts.length - 1] : "";
  return TABS.includes(last) ? last : "admin";
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  if (!ctx.hasAction("tenants.get")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("p", { class: "eyebrow", text: "Utilities" }),
        el("h1", { text: "Settings is planned" }),
        el("p", {
          class: "field-hint",
          text: "tenants.get is not in this role's manifest; the settings tabs appear when the tenant read is granted.",
        }),
      ),
      title: "Settings is planned",
    };
  }

  const tab = currentTab();
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading settings…" });
  const body = el("div", { class: "destination-body" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Utilities" }),
    el("h1", { text: "Settings" }),
    el("p", {
      class: "lede",
      text: "The tenant, its approval policies, its members and their roles. Every write goes through the action registry and refetches.",
    }),
    actions,
    status,
    body,
  );

  /**
   * @param {string} name
   * @param {string} label
   * @param {Record<string, unknown>} [initial]
   * @param {number|string} [ifMatch]
   */
  function button(name, label, initial = {}, ifMatch = undefined) {
    return el("button", {
      class: "button",
      type: "button",
      text: label,
      onClick: () => ctx.shell.openAction(name, { title: label, initial, ifMatch, onDone: load }),
    });
  }

  async function loadAdmin() {
    const tenant = (await ctx.api.invoke("tenants.get", {})).output;
    const settings = tenant?.settings === undefined ? undefined : JSON.stringify(tenant.settings, null, 2);
    body.append(
      el("div", { "data-testid": "settings-tenant" },
        el("h2", { text: "Tenant" }),
        el("dl", { class: "facts" },
          el("dt", { text: "Name" }), el("dd", { text: text(tenant, "name") }),
          el("dt", { text: "Slug" }), el("dd", { text: text(tenant, "slug") }),
          el("dt", { text: "Settings" }),
          el("dd", {}, el("pre", { class: "mono", text: settings ?? "—" })),
        ),
      ),
    );
    if (ctx.hasAction("tenants.update_settings")) {
      actions.append(button("tenants.update_settings", "Update settings", { settings }, tenant?.version));
    }
  }

  async function loadInference() {
    const policies = rows((await ctx.api.invoke("approval_policies.list", {})).output);
    if (policies.length === 0) {
      body.append(
        el("div", { class: "empty" },
          el("p", { text: "No approval policy. This is the empty state, not an error." }),
        ),
      );
    } else {
      body.append(
        table(
          "Approval policies",
          ["Name", "Kind", "Status"],
          policies.map((policy) => [text(policy, "name"), text(policy, "kind"), text(policy, "status")]),
          "settings-policies",
        ),
      );
    }
    if (ctx.hasAction("approval_policies.set")) {
      actions.append(button("approval_policies.set", "Set a policy"));
    }
  }

  /** @param {any} member */
  function memberActions(member) {
    const cell = el("div", {});
    if (ctx.hasAction("members.set_role")) {
      cell.append(
        el("button", {
          class: "button ghost",
          type: "button",
          text: "Change role",
          onClick: () =>
            ctx.shell.openAction("members.set_role", {
              title: "Change role",
              initial: { user_id: member.user_id, role: member.role },
              ifMatch: member.version,
              onDone: load,
            }),
        }),
      );
    }
    if (ctx.hasAction("members.remove")) {
      cell.append(
        el("button", {
          class: "button ghost",
          type: "button",
          text: "Remove",
          onClick: () =>
            ctx.shell.openAction("members.remove", {
              title: "Remove",
              initial: { user_id: member.user_id },
              onDone: load,
            }),
        }),
      );
    }
    return cell;
  }

  async function loadMembers() {
    const members = rows((await ctx.api.invoke("members.list", { limit: 100 })).output);
    if (members.length === 0) {
      body.append(
        el("div", { class: "empty" },
          el("p", { text: "No member yet. This is the empty state, not an error." }),
        ),
      );
    } else {
      body.append(
        table(
          "Members",
          ["Label", "Role", "Status", "Actions"],
          members.map((member) => [
            first(member, ["label", "display_name", "email"]),
            text(member, "role"),
            text(member, "status"),
            memberActions(member),
          ]),
          "settings-members",
        ),
      );
    }
  }

  async function loadRoles() {
    const policy = (await ctx.api.invoke("roles.policy_get", {})).output;
    const entries = rows(policy?.entries);
    const cells = rows(policy?.cells);
    if (entries.length > 0) {
      body.append(
        table(
          "Channel policy",
          ["Action", "Allowed"],
          entries.map((entry) => [text(entry, "action"), text(entry, "allowed")]),
          "settings-roles",
        ),
      );
    } else if (cells.length > 0) {
      body.append(
        table(
          "Role policy",
          ["Role", "Scope", "Allowed"],
          cells.map((cell) => [text(cell, "role"), text(cell, "scope"), text(cell, "allowed")]),
          "settings-roles",
        ),
      );
    } else {
      body.append(
        el("div", { class: "empty" },
          el("p", { text: "No role policy override. This is the empty state, not an error." }),
        ),
      );
    }
    if (ctx.hasAction("roles.policy_set")) {
      actions.append(button("roles.policy_set", "Set the policy"));
    }
  }

  async function load() {
    status.textContent = "Loading settings…";
    clear(actions);
    clear(body);
    try {
      if (tab === "inference") await loadInference();
      else if (tab === "members") await loadMembers();
      else if (tab === "roles") await loadRoles();
      else await loadAdmin();
      status.textContent = "";
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Settings" };
}
