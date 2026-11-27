// Clean URLs and HTML5 history on the game host's root. Back/forward restores the path; a
// signed-out deep link is kept by the auth module and restored after the callback.

import { resolve } from "./registry.js";

export class Router {
  /**
   * @param {(found: {row: any, params: Record<string,string>}|null, path: string) => void} onRoute
   */
  constructor(onRoute) {
    this.onRoute = onRoute;
    this.handlePop = () => this.resolve();
  }

  start() {
    window.addEventListener("popstate", this.handlePop);
    this.resolve();
  }

  stop() {
    window.removeEventListener("popstate", this.handlePop);
  }

  /** @param {string} [path] */
  resolve(path = window.location.pathname) {
    this.onRoute(resolve(path), path);
  }

  /**
   * @param {string} path
   * @param {{replace?: boolean}} [options]
   */
  navigate(path, options = {}) {
    if (options.replace) history.replaceState(null, "", path);
    else history.pushState(null, "", path);
    this.resolve(path);
  }
}
