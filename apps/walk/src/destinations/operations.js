// The Operations destination (S3): the cluster and host reads of the `ops.*` family, and the
// gated acts the operations job performs. Every read names a closed operation and answers with
// what the estate's own integration returned; a read the role cannot call is skipped and named,
// and one refused read never blanks its siblings.

import { el, clear } from "../dom.js";

/**
 * @typedef {{title: string, action: string, input: Record<string, unknown>, empty: string,
 *            labels: string[], testId: string, cells: (item: any) => Array<Node|string>,
 *            pick?: (output: any) => any[]}} OperationSection
 */

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

/** @param {string[]} labels @param {Array<Array<Node|string>>} bodyRows @param {string} testId */
function grid(labels, bodyRows, testId) {
  const node = el("table", { class: "table", "data-testid": testId });
  node.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const cells of bodyRows) {
    tbody.append(
      el("tr", {}, ...cells.map((cell) => (cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) })))),
    );
  }
  node.append(tbody);
  return el("div", {}, node);
}

/** @param {any} event */
function involved(event) {
  const object = event?.involvedObject;
  if (!object || typeof object !== "object") return "—";
  const kind = typeof object.kind === "string" && object.kind !== "" ? object.kind : "—";
  const name = typeof object.name === "string" && object.name !== "" ? object.name : "—";
  return `${kind}/${name}`;
}

/** @param {any} deployment */
function ready(deployment) {
  const status = deployment?.status;
  const spec = deployment?.spec;
  const readyCount = typeof status?.readyReplicas === "number" ? status.readyReplicas : 0;
  const total =
    typeof status?.replicas === "number"
      ? status.replicas
      : typeof spec?.replicas === "number"
        ? spec.replicas
        : null;
  return total === null ? "—" : `${readyCount}/${total}`;
}

/** @param {any} kustomization */
function fluxCondition(kustomization) {
  const conditions = kustomization?.status?.conditions;
  if (!Array.isArray(conditions)) return null;
  return conditions.find((condition) => condition?.type === "Ready") ?? null;
}

/** @returns {{element: HTMLElement, title: string}} */
function planned() {
  return {
    element: el(
      "section",
      { class: "destination" },
      el("h1", { text: "Operations is planned" }),
      el("p", {
        class: "field-hint",
        text: "ops.pods is not in this role's manifest; the screen appears when the operations family is granted.",
      }),
    ),
    title: "Operations is planned",
  };
}

/**
 * One section's read: the absent action, the loading line, the empty state and the refusal are
 * all states of this block alone.
 * @param {any} ctx
 * @param {HTMLElement} host
 * @param {OperationSection} spec
 */
async function loadSection(ctx, host, spec) {
  clear(host);
  if (!ctx.hasAction(spec.action)) {
    host.append(
      el("p", {
        class: "field-hint",
        text: `${spec.action} is not in this role's manifest; this section appears when the action is granted.`,
      }),
    );
    return;
  }
  host.append(el("p", { class: "loading", role: "status", text: `Loading ${spec.title.toLowerCase()}…` }));
  try {
    const result = await ctx.api.invoke(spec.action, spec.input);
    const items = spec.pick ? spec.pick(result.output) : rows(result.output);
    clear(host);
    if (items.length === 0) {
      host.append(el("div", { class: "empty" }, el("p", { text: spec.empty })));
      return;
    }
    host.append(grid(spec.labels, items.map(spec.cells), spec.testId));
  } catch (error) {
    clear(host);
    host.append(ctx.shell.problemPanel(error));
  }
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  if (!ctx.hasAction("ops.pods")) return planned();

  const section = el("section", { class: "destination" });
  const actions = el("div", { class: "destination-actions" });
  const body = el("div", { class: "destination-body" });
  section.append(
    el("p", { class: "eyebrow", text: "Operations" }),
    el("h1", { text: "Operations" }),
    el("p", {
      class: "lede",
      text: "Incidents, the cluster and host reads, and the gated acts. Every write is an engine action; a destructive one opens its approval gate and changes nothing until that gate is approved.",
    }),
    actions,
    body,
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

  for (const [name, label] of [
    ["ops.incident", "Open an incident"],
    ["ops.mitigate", "Mitigate"],
    ["ops.postmortem", "Postmortem"],
    ["ops.resolve", "Resolve"],
    ["ops.host_restart_unit", "Restart a unit"],
  ]) {
    if (ctx.hasAction(name)) actions.append(button(name, label));
  }

  /** @type {OperationSection[]} */
  const specs = [
    {
      title: "Pods",
      action: "ops.pods",
      input: { namespace: "conveyor", limit: 50 },
      empty: "No pod in the conveyor namespace.",
      labels: ["Name", "Phase"],
      testId: "operations-pods",
      cells: (pod) => [text(pod?.metadata, "name"), text(pod?.status, "phase")],
    },
    {
      title: "Events",
      action: "ops.events",
      input: { namespace: "conveyor", limit: 50 },
      empty: "No event in the conveyor namespace.",
      labels: ["Reason", "Object", "Message"],
      testId: "operations-events",
      cells: (event) => [text(event, "reason"), involved(event), text(event, "message")],
    },
    {
      title: "CI",
      action: "ops.ci_status",
      input: {},
      empty: "No CI run came back.",
      labels: ["Run", "Branch", "Status", "Conclusion"],
      testId: "operations-ci",
      pick: (output) => (Array.isArray(output?.runs) ? output.runs : []),
      cells: (run) => [
        text(run, "name"),
        text(run, "head_branch"),
        text(run, "status"),
        text(run, "conclusion"),
      ],
    },
    {
      title: "Deployments",
      action: "ops.deployments",
      input: { namespace: "conveyor" },
      empty: "No deployment in the conveyor namespace.",
      labels: ["Name", "Ready"],
      testId: "operations-deployments",
      cells: (deployment) => [text(deployment?.metadata, "name"), ready(deployment)],
    },
    {
      title: "Flux",
      action: "ops.flux_status",
      input: {},
      empty: "No Kustomization came back.",
      labels: ["Kustomization", "Ready", "Message"],
      testId: "operations-flux",
      cells: (kustomization) => {
        const condition = fluxCondition(kustomization);
        return [
          text(kustomization?.metadata, "name"),
          text(condition, "status"),
          text(condition, "message"),
        ];
      },
    },
  ];

  const hosts = specs.map((spec) => {
    const block = el("section", {}, el("h2", { text: spec.title }));
    const host = el("div", {});
    block.append(host);
    body.append(block);
    return { spec, host };
  });

  async function load() {
    await Promise.all(hosts.map(({ spec, host }) => loadSection(ctx, host, spec)));
  }

  await load();
  return { element: section, title: "Operations" };
}
