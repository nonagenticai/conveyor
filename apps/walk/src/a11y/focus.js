// Focus and announcement helpers. A sheet is a dialog: focus enters it on open, Tab stays
// inside, Escape closes, and focus returns to the opener (SPEC §8).

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * @param {HTMLElement} container
 * @returns {HTMLElement[]}
 */
export function focusables(container) {
  return /** @type {HTMLElement[]} */ (
    Array.from(container.querySelectorAll(FOCUSABLE)).filter(
      (node) => node instanceof HTMLElement && node.offsetParent !== null,
    )
  );
}

/**
 * Trap Tab inside a container. Returns the release function.
 * @param {HTMLElement} container
 * @param {() => void} onEscape
 */
export function trapFocus(container, onEscape) {
  const onKey = (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onEscape();
      return;
    }
    if (event.key !== "Tab") return;
    const items = focusables(container);
    if (items.length === 0) {
      event.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };
  container.addEventListener("keydown", onKey);
  return () => container.removeEventListener("keydown", onKey);
}

/** @param {HTMLElement|null} node */
export function focusFirst(node) {
  if (!node) return;
  const target = focusables(node)[0] ?? node;
  target.focus();
}

/** @param {string} message */
export function announce(message) {
  const host = document.getElementById("announcer");
  if (!host) return;
  host.textContent = "";
  // A second task boundary makes repeated identical announcements audible.
  setTimeout(() => {
    host.textContent = message;
  }, 30);
}
