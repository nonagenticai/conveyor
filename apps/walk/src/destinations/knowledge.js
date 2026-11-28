// The Knowledge destination (S3): `capabilities.list` supplies the capability a query is bound
// to, `knowledge.query` retrieves from that capability's pinned packs, and the lesson buttons
// carry a finding into the organisational memory. Reads fail through the server's own problem
// panel; an absent read is named and skipped rather than faked.

import { el, clear } from "../dom.js";

/** @param {any} value @returns {any[]} */
function rows(value) {
  if (Array.isArray(value?.items)) return value.items;
  return Array.isArray(value) ? value : [];
}

/** @param {any} row @param {...string} keys */
function text(row, ...keys) {
  for (const key of keys) {
    const value = row?.[key];
    if (typeof value === "string" && value !== "") return value;
    if (typeof value === "number") return String(value);
  }
  return "—";
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
  if (!ctx.hasAction("knowledge.query")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "Knowledge is planned" }),
        el("p", {
          class: "field-hint",
          text: "knowledge.query is not in this role's manifest; the screen appears when the knowledge family is granted.",
        }),
      ),
      title: "Knowledge is planned",
    };
  }

  const actions = el("div", { class: "destination-actions" });
  const capabilityHost = el("div", { class: "field" });
  const capabilityProblem = el("div");
  const results = el("div", { class: "destination-body" });
  const status = el("p", { class: "loading", role: "status" });
  const capabilitySelect = /** @type {HTMLSelectElement} */ (
    el("select", { "aria-label": "Capability" })
  );
  capabilitySelect.id = "knowledge-capability";
  /** The capability the select names; `""` until `capabilities.list` answers. */
  let selectedCapabilityId = "";
  capabilitySelect.addEventListener("change", () => {
    selectedCapabilityId = capabilitySelect.value;
  });

  const queryInput = /** @type {HTMLInputElement} */ (
    el("input", { type: "text", placeholder: "what the organisation knows", "aria-label": "Knowledge query" })
  );
  queryInput.id = "knowledge-query";

  if (ctx.hasAction("capabilities.list")) {
    capabilityHost.append(
      el("label", { class: "field-label", for: "knowledge-capability", text: "Capability" }),
      capabilitySelect,
    );
  } else {
    capabilityHost.append(
      el("p", {
        class: "field-hint",
        text: "capabilities.list is not in this role's manifest; the capability select is skipped and a query cannot name its capability.",
      }),
    );
  }

  const queryField = el(
    "div",
    { class: "field" },
    el("label", { class: "field-label", for: "knowledge-query", text: "Knowledge query" }),
    queryInput,
    el("button", { class: "button primary", type: "button", text: "Query", onClick: () => void query() }),
  );

  /** @param {string} name @param {string} label */
  function button(name, label) {
    return el("button", {
      class: "button",
      type: "button",
      text: label,
      onClick: () => ctx.shell.openAction(name, { title: label, onDone: load }),
    });
  }

  async function load() {
    if (!ctx.hasAction("capabilities.list")) return;
    status.textContent = "Loading capabilities…";
    clear(capabilityProblem);
    try {
      const result = await ctx.api.invoke("capabilities.list", { limit: 100 });
      const items = rows(result.output);
      const previous = selectedCapabilityId;
      clear(capabilitySelect);
      if (items.length === 0) {
        capabilitySelect.append(el("option", { value: "", text: "No capability in this tenant" }));
        selectedCapabilityId = "";
      } else {
        for (const item of items) {
          const id = typeof item?.id === "string" ? item.id : "";
          capabilitySelect.append(
            el("option", { value: id, text: `${text(item, "code", "key")} — ${text(item, "label", "name")}` }),
          );
        }
        const keep = previous !== "" && items.some((item) => item?.id === previous);
        capabilitySelect.value = keep
          ? previous
          : typeof items[0]?.id === "string"
            ? items[0].id
            : "";
        selectedCapabilityId = capabilitySelect.value;
      }
      status.textContent = "";
    } catch (error) {
      status.textContent = "";
      capabilityProblem.append(ctx.shell.problemPanel(error));
    }
  }

  async function query() {
    const question = queryInput.value.trim();
    clear(results);
    if (question === "") {
      status.textContent = "Name a knowledge query.";
      return;
    }
    if (selectedCapabilityId === "") {
      status.textContent = "Choose a capability; knowledge.query needs one.";
      return;
    }
    status.textContent = "Querying…";
    try {
      const result = await ctx.api.invoke("knowledge.query", {
        capability_id: selectedCapabilityId,
        query: question,
        top_k: 10,
      });
      status.textContent = "";
      const hits = Array.isArray(result.output?.hits) ? result.output.hits : rows(result.output);
      if (hits.length === 0) {
        results.append(el("p", { class: "empty", text: "No knowledge result for this query." }));
        return;
      }
      results.append(
        table(
          "Results",
          ["Source", "Ref", "Text"],
          hits.map((hit) => [
            text(hit, "source", "path", "url"),
            text(hit, "ref", "id", "chunk_id"),
            text(hit, "text", "content", "snippet"),
          ]),
          "knowledge-results",
        ),
      );
      const warnings = Array.isArray(result.output?.warnings)
        ? result.output.warnings.filter((warning) => typeof warning === "string")
        : [];
      if (result.output?.degraded) {
        results.append(
          el("p", {
            class: "field-hint",
            text: warnings.length > 0 ? `Degraded: ${warnings.join(" · ")}` : "Degraded: an optional pack was skipped.",
          }),
        );
      }
    } catch (error) {
      status.textContent = "";
      clear(results);
      results.append(ctx.shell.problemPanel(error));
    }
  }

  for (const [name, label] of [
    ["lessons.propose", "Propose a lesson"],
    ["lessons.review", "Review a lesson"],
    ["lessons.promote", "Promote a lesson"],
  ]) {
    if (ctx.hasAction(name)) actions.append(button(name, label));
  }

  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Knowledge & memory" }),
    el("h1", { text: "Knowledge" }),
    el("p", {
      class: "lede",
      text: "Retrieval from a capability's pinned knowledge packs, and the lesson lifecycle that turns a finding into organisational memory.",
    }),
    actions,
    capabilityHost,
    capabilityProblem,
    queryField,
    status,
    results,
  );

  await load();
  return { element: section, title: "Knowledge" };
}
