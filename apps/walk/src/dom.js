// Small DOM helpers. Everything server-derived is set as text: no innerHTML of server data
// (SPEC §9). Attributes are set through setAttribute, never by string concatenation.

/**
 * @param {string} tag
 * @param {Record<string, unknown>} [props]
 * @param {...(Node|string|null|undefined)} children
 * @returns {HTMLElement}
 */
export function el(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [name, value] of Object.entries(props)) {
    if (value === undefined || value === null || value === false) continue;
    if (name === "class") node.className = String(value);
    else if (name === "text") node.textContent = String(value);
    else if (name === "dataset") {
      for (const [key, item] of Object.entries(/** @type {Record<string, unknown>} */ (value))) {
        if (item !== undefined && item !== null) node.dataset[key] = String(item);
      }
    } else if (name.startsWith("on") && typeof value === "function") {
      node.addEventListener(name.slice(2).toLowerCase(), /** @type {EventListener} */ (value));
    } else if (name in node) {
      // Properties like `disabled`, `value`, `href` (on anchors), `role` fall through.
      node[name] = value;
    } else {
      node.setAttribute(name, String(value));
    }
  }
  for (const child of children) {
    if (child === undefined || child === null) continue;
    node.append(typeof child === "string" ? document.createTextNode(child) : child);
  }
  return node;
}

/** @param {Element} node */
export function clear(node) {
  while (node.firstChild) node.removeChild(node.firstChild);
}

/**
 * A labelled field wrapper.
 * @param {string} label
 * @param {HTMLElement} control
 * @param {{hint?: string, id?: string}} [options]
 */
export function field(label, control, options = {}) {
  const id = options.id ?? `f-${Math.random().toString(36).slice(2, 10)}`;
  control.id = id;
  const parts = [el("label", { class: "field-label", for: id }, label)];
  if (options.hint) parts.push(el("p", { class: "field-hint", text: options.hint }));
  parts.push(control);
  return el("div", { class: "field" }, ...parts);
}

/** @param {string} message @param {"info"|"error"|"success"} [kind] */
export function toast(message, kind = "info") {
  const host = document.getElementById("toasts");
  if (!host) return;
  const item = el("div", { class: `toast toast-${kind}`, role: "status", text: message });
  host.append(item);
  setTimeout(() => item.remove(), 6000);
}

/** @param {number} n @returns {string} */
export function plural(n, one, many) {
  return `${n} ${n === 1 ? one : many}`;
}
