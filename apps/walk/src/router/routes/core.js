// G0's own rows: the entrance and the Portfolio destination. The walk path IS the app path on
// the game host; the no-host dev shape serves the same tree under /walk/.
// The census parser reads these object literals; keep the key spelling exact.

export default [
  {
    walk: "/",
    app: "/",
    room: "portfolio",
    spec: "e2e/shell/entrance.spec.ts",
    load: () => import("../../destinations/portfolio.js"),
  },
  {
    walk: "/portfolio",
    app: "/portfolio",
    room: "portfolio",
    spec: "e2e/portfolio/portfolio.spec.ts",
    load: () => import("../../destinations/portfolio.js"),
  },
  {
    walk: "/portfolio/:programme_id",
    app: "/portfolio/:programme_id",
    room: "portfolio",
    spec: "e2e/portfolio/portfolio.spec.ts",
    load: () => import("../../destinations/portfolio.js"),
  },
];
