// The Authoring destination (S2): the intakes a converted source arrives in, the authoring
// assets drafted from them, their review approval and their publish. Reads are
// `authoring.intakes`/`authoring.assets`; every write goes through the action sheet and refetches.

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
  if (!ctx.hasAction("authoring.intakes")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "Authoring is planned" }),
        el("p", { class: "field-hint", text: "authoring.intakes is not in this role's manifest; the screen appears when the authoring family is granted." }),
      ),
      title: "Authoring is planned",
    };
  }

  const body = el("div", { class: "destination-body" });
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading intakes…" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Procedures & authoring" }),
    el("h1", { text: "Authoring" }),
    el("p", {
      class: "lede",
      text: "An intake is the brief a source arrives in; the assets drafted from it are reviewed against an approval and published into the procedure, standards or knowledge families.",
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
    status.textContent = "Loading intakes…";
    actions.replaceChildren();
    body.replaceChildren();
    try {
      const intakes = await ctx.api.invoke("authoring.intakes", { page: { limit: 100 } }).then((r) => rows(r.output));
      const assets = ctx.hasAction("authoring.assets")
        ? await ctx.api.invoke("authoring.assets", { page: { limit: 100 } }).then((r) => rows(r.output))
        : [];
      if (ctx.hasAction("authoring.intake")) actions.append(button("authoring.intake", "New intake"));

      status.textContent = intakes.length === 0 ? "" : `${intakes.length} intake${intakes.length === 1 ? "" : "s"}`;
      if (intakes.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No intake yet. This is the empty state, not an error." }),
            ctx.hasAction("authoring.intake")
              ? el("button", { class: "button", type: "button", text: "Open the first intake", onClick: () => ctx.shell.openAction("authoring.intake", { title: "New intake", onDone: load }) })
              : el("p", { class: "field-hint", text: "Your role cannot open an intake." }),
          ),
        );
      } else {
        body.append(
          table(
            "Intakes",
            ["Brief", "Status", "Version", "Created", "Actions"],
            intakes.map((intake) => [
              text(intake, "brief"),
              text(intake, "status"),
              num(intake, "version"),
              text(intake, "created_at"),
              (() => {
                const cells = el("td", {});
                if (ctx.hasAction("authoring.draft")) {
                  cells.append(button("authoring.draft", "Draft", { intake_id: intake.id }));
                }
                return cells;
              })(),
            ]),
            "authoring-intakes",
          ),
        );
      }
      if (assets.length > 0) {
        const assetRows = assets.map((asset) => [
          text(asset, "key"),
          text(asset, "kind"),
          text(asset, "status"),
          num(asset, "version"),
          (() => {
            const cells = el("td", {});
            if (ctx.hasAction("authoring.review") && asset.status === "draft") {
              cells.append(button("authoring.review", "Send for review", { asset_id: asset.id }));
            }
            if (ctx.hasAction("authoring.publish") && asset.status === "approved") {
              cells.append(button("authoring.publish", "Publish", { asset_id: asset.id }));
            }
            return cells;
          })(),
        ]);
        body.append(table("Assets", ["Key", "Kind", "Status", "Version", "Actions"], assetRows, "authoring-assets"));
      }
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Authoring" };
}
