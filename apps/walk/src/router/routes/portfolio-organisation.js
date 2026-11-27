// S1's rows (Portfolio & plan + Organisation). Each row is the census's walk/app/room/spec
// exactly; the destination module is loaded lazily (GEMBA-WALK-SPEC §6.1).

export default [
  {
    walk: "/organisation",
    app: "/organisation",
    room: "organisation",
    spec: "e2e/portfolio-organisation/organisation.spec.ts",
    load: () => import("../../destinations/organisation.js"),
  },
  {
    walk: "/project-types",
    app: "/project-types",
    room: "organisation",
    spec: "e2e/portfolio-organisation/project-types.spec.ts",
    load: () => import("../../destinations/project-types.js"),
  },
];
