// The ADM destination (S2, Govern & design): the architecture repository as `architecture.*`
// reads it — the adopted profile and its standard, the board's decisions, the contracts and the
// artefacts — and the phase, review, board, maturity and profile acts as writes. Every write
// goes through the action sheet and refetches.

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
  if (!ctx.hasAction("architecture.profile_get")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "ADM is planned" }),
        el("p", { class: "field-hint", text: "architecture.profile_get is not in this role's manifest; the screen appears when the architecture family is granted." }),
      ),
      title: "ADM is planned",
    };
  }

  const body = el("div", { class: "destination-body" });
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading the architecture repository…" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Govern & design" }),
    el("h1", { text: "ADM" }),
    el("p", {
      class: "lede",
      text: "The adopted ADM profile and its artefact standard, the Board's decisions, the delivery contracts awaiting a signature and the artefacts each phase produces. Every act below is an engine action; the Board decides its own gates.",
    }),
    actions,
    status,
    body,
  );

  /** @param {string} name @param {string} label @param {Record<string, unknown>} [initial] **/
  function button(name, label, initial = {}) {
    return el("button", {
      class: "button",
      type: "button",
      text: label,
      onClick: () => ctx.shell.openAction(name, { title: label, initial, onDone: load }),
    });
  }

  async function load() {
    status.textContent = "Loading the architecture repository…";
    actions.replaceChildren();
    body.replaceChildren();
    try {
      const profile = await ctx.api.invoke("architecture.profile_get", {}).then((r) => r.output ?? {});
      const board = await ctx.api.invoke("architecture.board_list", { page: { limit: 20 } }).then((r) => rows(r.output));
      const contracts = await ctx.api.invoke("architecture.contracts_list", { page: { limit: 50 } }).then((r) => rows(r.output));
      const artifacts = await ctx.api.invoke("architecture.artifacts_list", { page: { limit: 100 } }).then((r) => rows(r.output));

      if (ctx.hasAction("architecture.phases_advance")) actions.append(button("architecture.phases_advance", "Advance a phase"));
      if (ctx.hasAction("architecture.artifacts_create")) actions.append(button("architecture.artifacts_create", "New artefact"));
      if (ctx.hasAction("architecture.contracts_request")) actions.append(button("architecture.contracts_request", "Request a contract"));
      if (ctx.hasAction("architecture.reviews_record")) actions.append(button("architecture.reviews_record", "Record a review"));
      if (ctx.hasAction("architecture.board_open")) actions.append(button("architecture.board_open", "Open the Board"));
      if (ctx.hasAction("architecture.maturity_assess")) actions.append(button("architecture.maturity_assess", "Assess maturity"));
      if (ctx.hasAction("architecture.profile_tailor")) actions.append(button("architecture.profile_tailor", "Tailor the profile"));

      const profileRow = profile.profile ?? null;
      status.textContent = "";
      body.append(
        profileRow
          ? table(
              "The adopted profile",
              ["Key", "Name", "Status", "Adopted"],
              [[text(profileRow, "key"), text(profileRow, "name"), text(profileRow, "status"), text(profileRow, "adopted_at")]],
              "adm-profile",
            )
          : el("div", { class: "empty" }, el("p", { text: "No architecture profile is adopted; the standard below is the default the engine carries." })),
      );
      body.append(
        firstOrEmpty("Board decisions", board, "No Board decision yet.", "adm-board", ["Name", "Status"], (row) => [text(row, "name"), text(row, "status")]),
      );
      body.append(
        firstOrEmpty(
          "Delivery contracts",
          contracts,
          "No contract yet. A project requests one when its architecture is ready to be signed.",
          "adm-contracts",
          ["Title", "Kind", "Phase", "Status", "Approved by"],
          (row) => [text(row, "title"), text(row, "kind"), text(row, "phase_code"), text(row, "status"), text(row, "approved_by")],
        ),
      );
      body.append(
        firstOrEmpty(
          "Artefacts",
          artifacts,
          "No artefact yet. Each phase produces the profile's required artefacts.",
          "adm-artifacts",
          ["Title", "Kind", "Phase", "Plateau", "Status", "Version"],
          (row) => [text(row, "title"), text(row, "artifact_kind"), text(row, "phase_code"), text(row, "plateau"), text(row, "status"), num(row, "artifact_version")],
        ),
      );
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  /** @param {string} title @param {any[]} items @param {string} emptyText @param {string} testId @param {string[]} labels @param {(row:any)=>Array<Node|string>} cells */
  function firstOrEmpty(title, items, emptyText, testId, labels, cells) {
    if (items.length === 0) {
      return el("section", {}, el("h2", { text: title }), el("div", { class: "empty" }, el("p", { text: emptyText })));
    }
    return table(title, labels, items.map(cells), testId);
  }

  await load();
  return { element: section, title: "ADM" };
}
