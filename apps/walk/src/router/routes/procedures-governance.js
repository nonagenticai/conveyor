// S2's rows (Procedures & authoring + Govern & design). Each row is the census's
// walk/app/room/spec exactly; the destination module is loaded lazily (GEMBA-WALK-SPEC §6.1).
// Detail routes share their list's module and spec.

export default [
  {
    walk: "/procedures",
    app: "/procedures",
    room: "procedures",
    spec: "e2e/procedures-governance/procedures.spec.ts",
    load: () => import("../../destinations/procedures.js"),
  },
  {
    walk: "/build/processes",
    app: "/build/processes",
    room: "procedures",
    spec: "e2e/procedures-governance/processes.spec.ts",
    load: () => import("../../destinations/processes.js"),
  },
  {
    walk: "/build/processes/:process_id",
    app: "/build/processes/:process_id",
    room: "procedures",
    spec: "e2e/procedures-governance/processes.spec.ts",
    load: () => import("../../destinations/processes.js"),
  },
  {
    walk: "/build/process-map",
    app: "/build/process-map",
    room: "procedures",
    spec: "e2e/procedures-governance/process-map.spec.ts",
    load: () => import("../../destinations/process-map.js"),
  },
  {
    walk: "/build/process-map/:map_id",
    app: "/build/process-map/:map_id",
    room: "procedures",
    spec: "e2e/procedures-governance/process-map.spec.ts",
    load: () => import("../../destinations/process-map.js"),
  },
  {
    walk: "/authoring",
    app: "/authoring",
    room: "procedures",
    spec: "e2e/procedures-governance/authoring.spec.ts",
    load: () => import("../../destinations/authoring.js"),
  },
  {
    walk: "/design/adm",
    app: "/design/adm",
    room: "governance",
    spec: "e2e/procedures-governance/adm.spec.ts",
    load: () => import("../../destinations/adm.js"),
  },
  {
    walk: "/architecture",
    app: "/architecture",
    room: "governance",
    spec: "e2e/procedures-governance/architecture.spec.ts",
    load: () => import("../../destinations/architecture.js"),
  },
  {
    walk: "/design/requirements",
    app: "/design/requirements",
    room: "governance",
    spec: "e2e/procedures-governance/requirements.spec.ts",
    load: () => import("../../destinations/requirements.js"),
  },
  {
    walk: "/design/requirements/:id",
    app: "/design/requirements/:id",
    room: "governance",
    spec: "e2e/procedures-governance/requirements.spec.ts",
    load: () => import("../../destinations/requirements.js"),
  },
  {
    walk: "/design/models",
    app: "/design/models",
    room: "governance",
    spec: "e2e/procedures-governance/models.spec.ts",
    load: () => import("../../destinations/models.js"),
  },
  {
    walk: "/design/models/:model_id",
    app: "/design/models/:model_id",
    room: "governance",
    spec: "e2e/procedures-governance/models.spec.ts",
    load: () => import("../../destinations/models.js"),
  },
  {
    walk: "/copilot",
    app: "/copilot",
    room: "governance",
    spec: "e2e/procedures-governance/copilot.spec.ts",
    load: () => import("../../destinations/copilot.js"),
  },
];
