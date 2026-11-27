// The one route registry: each sibling owns its file, this file composes them in a fixed
// order and holds no destination rows of its own (SPEC §6.1).

import core from "./routes/core.js";
import portfolioOrganisation from "./routes/portfolio-organisation.js";
import proceduresGovernance from "./routes/procedures-governance.js";
import measureOperationsUtilities from "./routes/measure-operations-utilities.js";
import v1 from "./routes/v1.js";

export const routes = [
  ...core,
  ...portfolioOrganisation,
  ...proceduresGovernance,
  ...measureOperationsUtilities,
  ...v1,
];

/**
 * Split a pattern into segments. A `:name` segment matches one path segment.
 * @param {string} pattern
 */
export function segments(pattern) {
  return pattern.split("/").filter((part) => part !== "");
}

/**
 * Match a pathname against a row pattern, returning its params or null.
 * @param {string} pattern
 * @param {string} pathname
 * @returns {Record<string, string>|null}
 */
export function matchPath(pattern, pathname) {
  const want = segments(pattern);
  const have = segments(pathname);
  if (want.length !== have.length) return null;
  /** @type {Record<string, string>} */
  const params = {};
  for (let index = 0; index < want.length; index += 1) {
    const part = want[index];
    if (part.startsWith(":")) {
      if (have[index] === "") return null;
      params[part.slice(1)] = decodeURIComponent(have[index]);
    } else if (part !== have[index]) {
      return null;
    }
  }
  return params;
}

/**
 * The first registry row that matches, with its params.
 * @param {string} pathname
 */
export function resolve(pathname) {
  for (const row of routes) {
    const params = matchPath(row.walk, pathname);
    if (params) return { row, params };
  }
  return null;
}
