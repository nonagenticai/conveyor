/* Documents — the tracked design, plans, evidence and decisions, rendered from Markdown
   to standalone HTML by docs/design/scripts/build-docs.py. The index is a JS assignment
   (`docs/documents.js`) so the page works from file:// without a fetch. */
"use strict";
window.DocsPack = (function () {
  const esc = (value) =>
    String(value == null ? "" : value).replace(
      /[&<>"']/g,
      (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
    );
  const payload = () => {
    const data = window.CONVEYOR_DOCUMENTS;
    return data && data.schema === "design-docs/v1" ? data : null;
  };
  // The publishable platform outputs, rendered at publish time and indexed here. A clean
  // checkout carries the null placeholder; the group is then absent, not broken.
  const outputs = () => {
    const data = window.CONVEYOR_OUTPUTS;
    return data && data.schema === "design-outputs/v1" && Array.isArray(data.outputs)
      ? data.outputs
      : [];
  };
  const SECTIONS = ["Spec", "Plans", "Evidence", "Decisions", "Design", "Root"];

  function item(doc) {
    return `<li class="docs-item" data-title="${esc((doc.title + " " + doc.source).toLowerCase())}"><a href="${esc(doc.path)}"><span class="docs-title">${esc(doc.title)}</span><span class="docs-source mono">${esc(doc.source)}</span></a><span class="docs-section">${esc(doc.section)}</span></li>`;
  }

  function outputItem(doc) {
    const project = doc.project_key || doc.project_name || "";
    const when = String(doc.updated_at || "").slice(0, 10);
    return `<li class="docs-item" data-title="${esc((doc.title + " " + project).toLowerCase())}"><a href="${esc(doc.path)}"><span class="docs-title">${esc(doc.title)}</span><span class="docs-source mono">${esc(project)}${project && when ? " · " : ""}${esc(when)}</span></a><span class="docs-section">Platform output</span></li>`;
  }

  function groups(docs) {
    return SECTIONS.map((section) => {
      const rows = docs.filter((doc) => doc.section === section);
      if (!rows.length) return "";
      return `<section class="docs-group"><h2>${esc(section)}<span class="docs-count">${rows.length}</span></h2><ul class="docs-list">${rows.map(item).join("")}</ul></section>`;
    }).join("");
  }

  function page(api) {
    const data = payload();
    const docs = data ? data.documents.slice().sort((a, b) => a.source.localeCompare(b.source)) : [];
    const platform = outputs()
      .slice()
      .sort((a, b) => String(a.title).localeCompare(String(b.title)));
    const total = docs.length + platform.length;
    const platformGroup = platform.length
      ? `<section class="docs-group"><h2>Platform outputs<span class="docs-count">${platform.length}</span></h2><ul class="docs-list">${platform.map(outputItem).join("")}</ul></section>`
      : "";
    const tracked = data
      ? groups(docs)
      : `<div class="v2-gate">No tracked document index is loaded. Run <span class="mono">python3 docs/design/scripts/build-docs.py</span> to generate <span class="mono">docs/documents.js</span>.</div>`;
    const body =
      data || total
        ? `<div class="docs-toolbar"><input id="docs-search" type="search" placeholder="Filter documents by title or path" aria-label="Filter documents" /><span class="muted" id="docs-shown">${total} ${total === 1 ? "document" : "documents"}</span></div>${tracked}${platformGroup}`
        : tracked;
    return `<div class="page enterprise docs-page">${api.header(
      "Documents",
      "Every tracked design, plan, evidence and decision document, plus the platform outputs published from the conveyor, rendered to standalone HTML at build or publish time. Links to code resolve to the repository.",
      "",
      "REFERENCE / DOCUMENTS",
    )}<div class="v2-ribbon" role="note"><strong>GENERATED</strong><span>Markdown → HTML at build or publish time; no runtime renderer and no external request. The source link on each page opens the Markdown in the repository.</span><span class="v2-src">${
      data || total ? total + (total === 1 ? " document" : " documents") : "index absent"
    }</span></div>${body}</div>`;
  }

  document.addEventListener("input", (event) => {
    const input = event.target;
    if (!input || input.id !== "docs-search") return;
    const query = input.value.trim().toLowerCase();
    let shown = 0;
    document.querySelectorAll(".docs-item").forEach((row) => {
      const hit = !query || (row.getAttribute("data-title") || "").includes(query);
      row.hidden = !hit;
      if (hit) shown += 1;
    });
    document.querySelectorAll(".docs-group").forEach((group) => {
      group.hidden = !group.querySelector(".docs-item:not([hidden])");
    });
    const counter = document.getElementById("docs-shown");
    if (counter) counter.textContent = shown + (shown === 1 ? " document" : " documents");
  });

  return { page };
})();
