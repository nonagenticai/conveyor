// The Prompt audit destination (S3): `prompt_audits.*` and `prompt_findings.*` as the app reads
// them. An audit inventories the selected surfaces and persists its findings; a finding is
// decided once, by the person the sheet asks, and the decision is shown with its status.

import { el, clear } from "../dom.js";

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

/** @param {unknown} value */
function shortId(value) {
  return typeof value === "string" && value !== "" ? value.slice(0, 8) : "—";
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

/** @param {string} action */
function note(action) {
  return el("p", {
    class: "field-hint",
    text: `${action} is not in this role's manifest; this section appears when the action is granted.`,
  });
}

/** @returns {{element: HTMLElement, title: string}} */
function planned() {
  return {
    element: el(
      "section",
      { class: "destination" },
      el("h1", { text: "Prompt audit is planned" }),
      el("p", {
        class: "field-hint",
        text: "prompt_audits.list is not in this role's manifest; the screen appears when the prompt-audit family is granted.",
      }),
    ),
    title: "Prompt audit is planned",
  };
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  if (!ctx.hasAction("prompt_audits.list")) return planned();

  const section = el("section", { class: "destination" });
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading prompt audits…" });
  const body = el("div", { class: "destination-body" });
  section.append(
    el("p", { class: "eyebrow", text: "Measure & improve" }),
    el("h1", { text: "Prompt audit" }),
    el("p", {
      class: "lede",
      text: "The audit scans the selected surfaces for obsolete prompt text, keeps a finding for each, and records the decision a person makes about it.",
    }),
    actions,
    status,
    body,
  );

  async function load() {
    status.textContent = "Loading prompt audits…";
    clear(actions);
    clear(body);
    if (ctx.hasAction("prompt_audits.start")) {
      actions.append(
        el("button", {
          class: "button primary",
          type: "button",
          text: "Start an audit",
          onClick: () =>
            ctx.shell.openAction("prompt_audits.start", {
              title: "Start an audit",
              onDone: load,
            }),
        }),
      );
    }
    try {
      const result = await ctx.api.invoke("prompt_audits.list", { limit: 100 });
      const audits = rows(result.output);
      status.textContent = audits.length === 0 ? "" : `${audits.length} audit${audits.length === 1 ? "" : "s"}`;
      if (audits.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No audit has run in this tenant yet." }),
          ),
        );
      } else {
        body.append(
          table(
            "Audits",
            ["Audit", "Status", "Findings", "Model", "When"],
            audits.map((audit) => [
              shortId(audit?.id),
              text(audit, "status"),
              num(audit, "finding_count"),
              text(audit, "target_model"),
              text(audit, "created_at"),
            ]),
            "prompt-audits",
          ),
        );
      }
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
    if (!ctx.hasAction("prompt_findings.list")) {
      body.append(note("prompt_findings.list"));
      return;
    }
    try {
      const result = await ctx.api.invoke("prompt_findings.list", { limit: 100 });
      const findings = rows(result.output);
      if (findings.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("h2", { text: "Nothing to report" }),
            el("p", {
              text: "This audit produced no findings. Running an audit inventories the selected surfaces and reports what is stale, duplicated or unusable.",
            }),
          ),
        );
        return;
      }
      body.append(
        table(
          "Findings",
          ["Finding", "Location", "Pattern", "Confidence", "Proposed", "Status", ""],
          findings.map((finding) => [
            shortId(finding?.id),
            text(finding, "location"),
            text(finding, "pattern"),
            text(finding, "confidence"),
            text(finding, "action"),
            text(finding, "status"),
            finding?.status === "open" && ctx.hasAction("prompt_findings.decide")
              ? el("button", {
                  class: "button ghost",
                  type: "button",
                  text: "Decide",
                  onClick: () =>
                    ctx.shell.openAction("prompt_findings.decide", {
                      title: `Decide ${shortId(finding.id)}`,
                      initial: { finding_id: finding.id },
                      onDone: load,
                    }),
                })
              : "",
          ]),
          "prompt-findings",
        ),
      );
    } catch (error) {
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Prompt audit" };
}
