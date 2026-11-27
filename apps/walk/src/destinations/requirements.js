// The Requirements destination (S2, Govern & design): `requirements.list` and
// `requirements.get` over the governance asset family, the trace links around a requirement, and
// the create/update/transition writes. A detail route renders one requirement; a transition is
// the state change, an edit carries the row version as If-Match.

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
  if (!ctx.hasAction("requirements.list")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "Requirements is planned" }),
        el("p", { class: "field-hint", text: "requirements.list is not in this role's manifest; the screen appears when the governance family is granted." }),
      ),
      title: "Requirements is planned",
    };
  }
  return ctx.params?.id ? detail(ctx, ctx.params.id) : list(ctx);
}

/** @param {any} ctx @param {string} id */
async function detail(ctx, id) {
  const section = el("section", { class: "destination" });
  try {
    const result = await ctx.api.invoke("requirements.get", { id });
    const requirement = result.output ?? {};
    const links = ctx.hasAction("trace_links.list")
      ? await ctx.api.invoke("trace_links.list", { page: { limit: 50 } }).then((r) => rows(r.output).filter((link) => link.from_id === id || link.to_id === id)).catch(() => [])
      : [];
    const actions = el("div", { class: "destination-actions" });
    if (ctx.hasAction("requirements.update")) {
      actions.append(
        el("button", {
          class: "button",
          type: "button",
          text: "Edit",
          onClick: () =>
            ctx.shell.openAction("requirements.update", {
              title: `Edit ${text(requirement, "title")}`,
              initial: { id },
              ifMatch: requirement.version,
              onDone: () => detail(ctx, id),
            }),
        }),
      );
    }
    if (ctx.hasAction("requirements.transition")) {
      actions.append(
        el("button", {
          class: "button primary",
          type: "button",
          text: "Transition",
          onClick: () => ctx.shell.openAction("requirements.transition", { title: "Transition", initial: { id }, onDone: () => detail(ctx, id) }),
        }),
      );
    }
    section.append(
      el("p", { class: "eyebrow" }, el("a", { href: "/design/requirements", text: "← All requirements" })),
      el("h1", { text: text(requirement, "title") }),
      el("p", { class: "lede", text: text(requirement, "body") }),
      el("dl", { class: "facts" },
        el("dt", { text: "Key" }), el("dd", { text: text(requirement, "key") }),
        el("dt", { text: "Status" }), el("dd", { text: text(requirement, "status") }),
        el("dt", { text: "Version" }), el("dd", { text: num(requirement, "version") }),
        el("dt", { text: "Owner" }), el("dd", { text: text(requirement, "owner_user_id") }),
        el("dt", { text: "Phase" }), el("dd", { text: text(requirement, "phase_code") }),
      ),
      actions,
    );
    if (links.length > 0) {
      section.append(table("Trace links", ["From", "To", "Kind"], links.map((link) => [text(link, "from_type"), text(link, "to_type"), text(link, "kind")]), "requirements-links"));
    }
    return { element: section, title: text(requirement, "title") };
  } catch (error) {
    section.replaceChildren(el("p", { class: "eyebrow" }, el("a", { href: "/design/requirements", text: "← All requirements" })), ctx.shell.problemPanel(error));
    return { element: section, title: "Requirement" };
  }
}

/** @param {any} ctx */
async function list(ctx) {
  const body = el("div", { class: "destination-body" });
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading requirements…" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Govern & design" }),
    el("h1", { text: "Requirements" }),
    el("p", { class: "lede", text: "The requirements the architecture answers, each with its status, version, acceptance and trace links." }),
    actions,
    status,
    body,
  );

  async function load() {
    status.textContent = "Loading requirements…";
    actions.replaceChildren();
    body.replaceChildren();
    try {
      const requirements = await ctx.api.invoke("requirements.list", { page: { limit: 100 } }).then((r) => rows(r.output));
      if (ctx.hasAction("requirements.create")) {
        actions.append(el("button", { class: "button primary", type: "button", text: "New requirement", onClick: () => ctx.shell.openAction("requirements.create", { title: "New requirement", onDone: load }) }));
      }
      status.textContent = requirements.length === 0 ? "" : `${requirements.length} requirement${requirements.length === 1 ? "" : "s"}`;
      if (requirements.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No requirement yet. This is the empty state, not an error." }),
            ctx.hasAction("requirements.create")
              ? el("button", { class: "button", type: "button", text: "Create the first requirement", onClick: () => ctx.shell.openAction("requirements.create", { title: "New requirement", onDone: load }) })
              : el("p", { class: "field-hint", text: "Your role cannot create requirements." }),
          ),
        );
        return;
      }
      const bodyRows = requirements.map((requirement) => [
        el("a", { href: `/design/requirements/${requirement.id}`, text: text(requirement, "title") }),
        text(requirement, "key"),
        text(requirement, "status"),
        num(requirement, "version"),
        text(requirement, "phase_code"),
      ]);
      body.append(table("Requirements", ["Title", "Key", "Status", "Version", "Phase"], bodyRows, "requirements-list"));
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Requirements" };
}
