// The shell: HUD, map, palette, sheets, toasts, the router and the world entrance. Every
// capability has a DOM path; the canvas is decorative and never the only way in (SPEC §8).

import { el, clear, toast } from "../dom.js";
import { renderSchemaForm } from "../forms/schema.js";
import { ProblemError, problemLine } from "../api/problem.js";
import { announce, focusFirst, trapFocus } from "../a11y/focus.js";
import { Router } from "../router/index.js";

export class Shell {
  /**
   * @param {{auth: any, api: any, routes: any[], worldData: any, world: any}} deps
   */
  constructor({ auth, api, routes, worldData, world }) {
    this.auth = auth;
    this.api = api;
    this.routes = routes;
    this.worldData = worldData;
    this.world = world;
    this.manifest = [];
    this.router = new Router((found, path) => this.renderRoute(found, path));
    this.hud = /** @type {HTMLElement} */ (document.getElementById("hud"));
    this.map = /** @type {HTMLElement} */ (document.getElementById("map"));
    this.screen = /** @type {HTMLElement} */ (document.getElementById("screen"));
    this.sheetHost = /** @type {HTMLElement} */ (document.getElementById("sheet-host"));
  }

  async start() {
    if (!this.auth.signedIn()) {
      this.renderSignIn();
      return;
    }
    try {
      this.manifest = await this.api.manifest();
    } catch (error) {
      this.renderHud();
      this.renderMap();
      clear(this.screen);
      this.screen.append(this.problemPanel(error));
      return;
    }
    this.renderSession();
    this.bindPaletteShortcut();
  }

  /**
   * The Palette's keyboard path. The button's tooltip promises Ctrl/Cmd+K, so the shortcut and
   * the tooltip are one thing (#914): remove this handler and the title goes with it.
   *
   * A field keeps its keystrokes: the shortcut is ignored while focus is in an input, a textarea,
   * a select or anything contenteditable, so typing a literal "k" into a form is never a command.
   * Bound once, on the signed-in path only — before the manifest loads the palette has nothing
   * to list.
   */
  bindPaletteShortcut() {
    if (this.paletteShortcutBound) return;
    this.paletteShortcutBound = true;
    window.addEventListener("keydown", (event) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      if (event.key !== "k" && event.key !== "K") return;
      const active = document.activeElement;
      if (active instanceof HTMLElement) {
        const tag = active.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || active.isContentEditable) {
          return;
        }
      }
      event.preventDefault();
      this.openPalette();
    });
  }

  /** The same shell, rebuilt after a tenant change or a return from a sheet. */
  renderSession() {
    document.getElementById("world")?.classList.remove("hidden");
    this.renderHud();
    this.renderMap();
    this.world?.mount?.();
    this.router.stop();
    this.router.start();
  }

  renderSignIn() {
    document.getElementById("world")?.classList.add("hidden");
    clear(this.hud);
    this.hud.append(
      el("div", { class: "brand" }, el("span", { class: "brand-mark", text: "◆" }),
        el("span", { text: "Conveyor · Gemba Walk" })),
    );
    clear(this.map);
    clear(this.screen);
    const modes = this.auth.modes();
    const returnPath = window.location.pathname + window.location.search;
    const body = el("section", { class: "signin" });
    body.append(
      el("p", { class: "eyebrow", text: "The Gemba Walk" }),
      el("h1", { text: "Sign in" }),
      el("p", { class: "lede", text: "The walk reads and writes the same records as the operator console." }),
    );
    if (this.auth.error) body.append(el("p", { class: "problem", role: "alert", text: this.auth.error }));
    if (modes.oidc) {
      body.append(
        el("button", {
          class: "button primary",
          type: "button",
          text: "Continue with Keycloak",
          onClick: () => this.auth.signIn(returnPath),
        }),
      );
    }
    if (modes.dev) {
      const login = /** @type {HTMLInputElement} */ (
        el("input", { type: "text", value: "alice", "aria-label": "Dev login" })
      );
      body.append(
        el("div", { class: "dev-login" },
          el("label", { class: "field-label", for: "dev-login", text: "Development cookie" }),
          login,
          el("button", {
            class: "button",
            type: "button",
            text: "Continue as dev user",
            onClick: () => this.auth.devLogin(login.value || "alice", returnPath),
          }),
        ),
      );
    }
    if (!modes.oidc && !modes.dev) {
      body.append(el("p", { class: "problem", text: "This origin has no sign-in configured." }));
    }
    this.screen.append(body);
    focusFirst(body);
  }

  renderHud() {
    clear(this.hud);
    const principal = this.auth.principal();
    const memberships = this.auth.memberships();
    const active = this.auth.me?.active_tenant?.tenant_id ?? "";
    const tenant = /** @type {HTMLSelectElement} */ (
      el("select", { class: "tenant", "aria-label": "Tenant" })
    );
    tenant.append(el("option", { value: "", text: "Choose tenant" }));
    for (const membership of memberships) {
      const option = /** @type {HTMLOptionElement} */ (
        el("option", {
          value: membership.tenant_id,
          text: `${membership.name} — ${membership.role}`,
        })
      );
      if (membership.tenant_id === active) option.selected = true;
      tenant.append(option);
    }
    tenant.addEventListener("change", async () => {
      await this.auth.selectTenant(tenant.value || null);
      this.renderSession();
    });
    this.hud.append(
      el("div", { class: "brand" }, el("span", { class: "brand-mark", text: "◆" }),
        el("span", { text: "Conveyor · Gemba Walk" })),
      el("div", { class: "hud-actions" },
        el("button", {
          class: "button",
          type: "button",
          text: "Palette",
          title: "Search routes and actions (Ctrl/Cmd + K)",
          onClick: () => this.openPalette(),
        }),
        tenant,
        el("span", { class: "who", text: principal ? principal.display_name : "signed in" }),
        el("button", {
          class: "button ghost",
          type: "button",
          text: "Sign out",
          onClick: () => this.auth.signOut(),
        }),
      ),
    );
  }

  renderMap() {
    clear(this.map);
    this.map.append(el("h2", { class: "map-title", text: "Rooms" }));
    for (const room of this.worldData.rooms) {
      this.map.append(
        el("button", {
          class: "map-room",
          type: "button",
          text: room.label,
          onClick: () => this.openRoom(room.id),
        }),
      );
    }
  }

  /** @param {{row: any, params: Record<string,string>}|null} found @param {string} path */
  async renderRoute(found, path) {
    clear(this.screen);
    if (!found) {
      this.screen.append(
        el("section", { class: "not-found" },
          el("h1", { text: "Not found" }),
          el("p", { text: `No walk destination for ${path}.` }),
          el("a", { href: "/", text: "Back to the entrance" }),
        ),
      );
      return;
    }
    this.screen.append(el("p", { class: "loading", role: "status", text: "Loading…" }));
    // If the user has already focused a control elsewhere (the room map, the HUD) while the
    // destination loaded, moving focus into the new screen would steal their place.
    const before = document.activeElement;
    try {
      const module = await found.row.load();
      const view = await module.render(this.context(found));
      clear(this.screen);
      this.screen.append(view.element);
      document.title = `${view.title} · Gemba Walk`;
      const userMoved =
        before instanceof HTMLElement &&
        before !== document.body &&
        document.contains(before) &&
        !this.screen.contains(before);
      if (!userMoved) focusFirst(view.element);
      announce(view.title);
    } catch (error) {
      clear(this.screen);
      this.screen.append(this.problemPanel(error));
    }
  }

  /** @param {{row: any, params: Record<string,string>}} found */
  context(found) {
    return {
      auth: this.auth,
      api: this.api,
      shell: this,
      manifest: this.manifest,
      params: found.params,
      row: found.row,
      worldData: this.worldData,
      navigate: (path) => this.router.navigate(path),
      hasAction: (name) => this.manifest.some((entry) => entry.name === name),
    };
  }

  /** @param {unknown} error */
  problemPanel(error) {
    const panel = el("section", { class: "problem", role: "alert" });
    if (error instanceof ProblemError) {
      panel.append(el("h2", { text: problemLine(error.problem) || "The request was refused" }));
      if (error.problem.fields.length > 0) {
        const list = el("ul", { class: "problem-fields" });
        for (const item of error.problem.fields) {
          list.append(el("li", { text: `${item.field}: ${item.detail}` }));
        }
        panel.append(list);
      }
      if (error.problem.status === 401) {
        panel.append(el("p", { text: "The session expired. Sign in again." }));
        panel.append(
          el("button", {
            class: "button",
            type: "button",
            text: "Sign in",
            onClick: () => this.auth.signIn(window.location.pathname),
          }),
        );
      }
    } else {
      panel.append(el("h2", { text: error instanceof Error ? error.message : String(error) }));
    }
    return panel;
  }

  /**
   * Open a modal sheet. `body` is an element; `onClose` runs once, after focus returns.
   * @param {{title: string, body: HTMLElement, onClose?: () => void}} options
   */
  openSheet({ title, body, onClose }) {
    const opener = document.activeElement;
    const backdrop = el("div", { class: "sheet-backdrop" });
    const close = () => {
      release();
      backdrop.remove();
      if (opener instanceof HTMLElement) opener.focus();
      onClose?.();
    };
    const dialog = el("section", {
      class: "sheet",
      role: "dialog",
      "aria-modal": "true",
      "aria-label": title,
    });
    dialog.append(
      el("header", { class: "sheet-head" },
        el("h2", { text: title }),
        el("button", { class: "button ghost", type: "button", text: "Close", onClick: close }),
      ),
      body,
    );
    backdrop.append(dialog);
    backdrop.addEventListener("click", (event) => {
      if (event.target === backdrop) close();
    });
    this.sheetHost.append(backdrop);
    const release = trapFocus(dialog, close);
    focusFirst(dialog);
    return { close, dialog };
  }

  /**
   * The schema-driven action sheet. A destructive action is never auto-approved: the caller
   * must supply the approval id, and the server's refusal is shown verbatim.
   * @param {string} name
   * @param {{initial?: Record<string, unknown>, ifMatch?: number|string,
   *          title?: string, onDone?: () => void|Promise<void>}} [options]
   */
  async openAction(name, options = {}) {
    let describe;
    try {
      describe = await this.api.describe(name);
    } catch (error) {
      const sheet = this.openSheet({ title: name, body: this.problemPanel(error) });
      return sheet;
    }
    const destructive = describe.effect === "destructive";
    let approvalId = "";
    const form = renderSchemaForm(describe.input, {
      initial: options.initial,
      submitLabel: destructive ? "Submit with approval" : "Submit",
      onSubmit: async (values) => {
        if (destructive && approvalId.trim() === "") {
          throw new Error("an approval id is required for a destructive action");
        }
        const isCreate = /\.create$/.test(name);
        const result = await this.api.invoke(name, values, {
          ifMatch: options.ifMatch,
          idempotencyKey: isCreate ? crypto.randomUUID() : undefined,
          approvalId: destructive ? approvalId.trim() : undefined,
        });
        toast(`${name} succeeded`, "success");
        sheet.close();
        await options.onDone?.();
        return result;
      },
    });
    const body = el("div", { class: "sheet-body" });
    if (typeof describe.input === "object" && describe.input) {
      body.append(
        el("p", { class: "sheet-meta" },
          `${describe.effect ?? "write"} · ${describe.tenancy ?? "tenant"}`,
        ),
      );
    }
    if (destructive) {
      const approval = /** @type {HTMLInputElement} */ (
        el("input", { type: "text", placeholder: "approval id", "aria-label": "Approval id" })
      );
      approval.addEventListener("input", () => {
        approvalId = approval.value;
      });
      body.append(
        el("div", { class: "field danger" },
          el("label", { class: "field-label", for: "approval-id", text: "Approval id (required)" }),
          approval,
          el("p", { class: "field-hint", text: "The walk never acquires an approval on your behalf." }),
        ),
      );
    }
    body.append(form.element);
    const sheet = this.openSheet({ title: options.title ?? name, body });
    return sheet;
  }

  openPalette() {
    const routes = this.routes.map((row) => ({ row, kind: "route" }));
    const actions = this.manifest.map((entry) => ({ entry, kind: "action" }));
    const results = el("div", { class: "palette-results" });
    const search = /** @type {HTMLInputElement} */ (
      el("input", { type: "search", placeholder: "Route or action", "aria-label": "Search" })
    );
    const render = () => {
      clear(results);
      const query = search.value.trim().toLowerCase();
      for (const item of routes) {
        const text = `${item.row.walk}  ${item.row.room}`;
        if (query && !text.toLowerCase().includes(query)) continue;
        results.append(
          el("button", {
            class: "palette-row",
            type: "button",
            text,
            onClick: () => {
              sheet.close();
              this.router.navigate(item.row.walk);
            },
          }),
        );
      }
      for (const item of actions) {
        if (query && !item.entry.name.toLowerCase().includes(query)) continue;
        results.append(
          el("button", {
            class: "palette-row",
            type: "button",
            text: `${item.entry.name}  ·  ${item.entry.kind ?? "action"}`,
            onClick: () => {
              sheet.close();
              void this.openAction(item.entry.name);
            },
          }),
        );
      }
    };
    search.addEventListener("input", render);
    const body = el("div", { class: "palette" }, search, results);
    const sheet = this.openSheet({ title: "Palette", body });
    render();
  }

  /**
   * A wall selected in the world. A live destination navigates; an absent one says so and
   * names the owning sibling — the world never fakes a screen (SPEC §4).
   * @param {any} wall
   */
  openWall(wall) {
    if (this.routes.some((row) => row.walk === wall.walk)) {
      this.router.navigate(wall.walk);
      return;
    }
    this.openSheet({
      title: wall.view,
      body: el("div", { class: "room-sheet" },
        el("p", { text: `Not yet live — ${wall.owner}.` }),
        el("p", { class: "field-hint", text: "This destination is delivered by its owning sibling; the route is already registered." }),
      ),
    });
  }

  /** @param {string} roomId */
  openRoom(roomId) {
    const room = this.worldData.rooms.find((item) => item.id === roomId);
    const walls = this.worldData.walls.filter((wall) => wall.room === roomId);
    const body = el("div", { class: "room-sheet" });
    body.append(el("p", { class: "sheet-meta", text: `${walls.length} destinations` }));
    for (const wall of walls) {
      const live = this.routes.some((row) => row.walk === wall.walk);
      body.append(
        el("button", {
          class: `wall-row${live ? "" : " not-live"}`,
          type: "button",
          disabled: !live,
          text: live ? wall.view : `${wall.view} — not yet live (${wall.owner})`,
          onClick: live
            ? () => {
                sheet.close();
                this.router.navigate(wall.walk);
              }
            : undefined,
        }),
      );
    }
    const sheet = this.openSheet({
      title: room ? room.label : roomId,
      body,
    });
    return sheet;
  }
}
