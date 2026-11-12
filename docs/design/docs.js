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
  const SECTIONS = ["Spec", "Plans", "Evidence", "Decisions", "Design", "Root"];

  function item(doc) {
    return `<li class="docs-item" data-title="${esc((doc.title + " " + doc.source).toLowerCase())}"><a href="${esc(doc.path)}"><span class="docs-title">${esc(doc.title)}</span><span class="docs-source mono">${esc(doc.source)}</span></a><span class="docs-section">${esc(doc.section)}</span></li>`;
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
    const body = data
      ? `<div class="docs-toolbar"><input id="docs-search" type="search" placeholder="Filter documents by title or path" aria-label="Filter documents" /><span class="muted" id="docs-shown">${docs.length} documents</span></div>${groups(docs)}`
      : `<div class="v2-gate">No document index is loaded. Run <span class="mono">python3 docs/design/scripts/build-docs.py</span> to generate <span class="mono">docs/documents.js</span>.</div>`;
    return `<div class="page enterprise docs-page">${api.header(
      "Documents",
      "Every tracked design, plan, evidence and decision document, rendered from Markdown to standalone HTML at build time. Links to code resolve to the repository.",
      "",
      "REFERENCE / DOCUMENTS",
    )}<div class="v2-ribbon" role="note"><strong>GENERATED</strong><span>Markdown → HTML at build time; no runtime renderer and no external request. The source link on each page opens the Markdown in the repository.</span><span class="v2-src">${
      data ? docs.length + " documents" : "index absent"
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
