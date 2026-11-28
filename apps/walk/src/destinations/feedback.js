// The Feedback destination (#888, the post-freeze surface from #842): the tenant's own
// submissions and the write that captures the route it was sent from. Reads fail through the
// server's problem panel; an empty list is the empty state, never a fabricated row.

import { el, clear } from "../dom.js";

/** @param {any} value @returns {any[]} */
function rows(value) {
  if (Array.isArray(value?.items)) return value.items;
  return Array.isArray(value) ? value : [];
}

/** @param {any} row @param {...string} keys @returns {string} */
function text(row, ...keys) {
  for (const key of keys) {
    const value = row?.[key];
    if (typeof value === "string" && value !== "") return value;
    if (typeof value === "number" || typeof value === "boolean") return String(value);
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
      el(
        "tr",
        {},
        ...cells.map((cell) =>
          cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) }),
        ),
      ),
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
  if (!ctx.hasAction("feedback.list")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("p", { class: "eyebrow", text: "Utilities" }),
        el("h1", { text: "Feedback is planned" }),
        el("p", {
          class: "field-hint",
          text: "feedback.list is not in this role's manifest; the screen appears when the feedback family is granted.",
        }),
      ),
      title: "Feedback is planned",
    };
  }

  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status" });
  const body = el("div", { class: "destination-body" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Utilities" }),
    el("h1", { text: "Feedback" }),
    el("p", {
      class: "lede",
      text: "Report a bug or request a feature for the platform itself. The operator reads every tenant's submissions; this page shows only your tenant's.",
    }),
    actions,
    status,
    body,
  );

  if (ctx.hasAction("feedback.create")) {
    actions.append(
      el("button", {
        class: "button primary",
        type: "button",
        text: "Send feedback",
        onClick: () =>
          ctx.shell.openAction("feedback.create", {
            title: "Send feedback",
            initial: { route: window.location.pathname },
            onDone: load,
          }),
      }),
    );
  }

  async function load() {
    status.textContent = "Loading feedback…";
    clear(body);
    try {
      const result = await ctx.api.invoke("feedback.list", { limit: 100 });
      const items = rows(result.output);
      status.textContent = "";
      if (items.length === 0) {
        body.append(el("p", { class: "empty", text: "No feedback yet. Yours will appear here." }));
        return;
      }
      body.append(
        table(
          "Your tenant's submissions",
          ["Kind", "Status", "Title", "Sent from", "When"],
          items.map((row) => [
            text(row, "kind"),
            text(row, "status"),
            text(row, "title"),
            text(row, "route"),
            text(row, "created_at"),
          ]),
          "feedback-rows",
        ),
      );
    } catch (error) {
      status.textContent = "";
      clear(body);
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Feedback" };
}
