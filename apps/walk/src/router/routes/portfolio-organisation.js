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
  {
    walk: "/agents",
    app: "/agents",
    room: "organisation",
    spec: "e2e/portfolio-organisation/agents.spec.ts",
    load: () => import("../../destinations/agents.js"),
  },
  {
    walk: "/jobs",
    app: "/jobs",
    room: "organisation",
    spec: "e2e/portfolio-organisation/jobs.spec.ts",
    load: () => import("../../destinations/jobs.js"),
  },
  {
    walk: "/runs",
    app: "/runs",
    room: "portfolio",
    spec: "e2e/portfolio-organisation/runs.spec.ts",
    load: () => import("../../destinations/runs.js"),
  },
  {
    walk: "/runs/:run_id",
    app: "/runs/:run_id",
    room: "portfolio",
    spec: "e2e/portfolio-organisation/runs.spec.ts",
    load: () => import("../../destinations/runs.js"),
  },
  {
    walk: "/tools",
    app: "/tools",
    room: "organisation",
    spec: "e2e/portfolio-organisation/tools.spec.ts",
    load: () => import("../../destinations/tools.js"),
  },
];
