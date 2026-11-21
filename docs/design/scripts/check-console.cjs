#!/usr/bin/env node
/*
 * check-console.cjs — the operator console boundary.
 *
 * Two stub servers serve the same pack:
 *   1. no /config.json      -> the homepage/file:// shape: read-only, ZERO /api requests,
 *                              no sign-in control, no write control.
 *   2. /config.json + a stub API -> the app shape: the console signs in (a loopback-only
 *                              `console_dev_token`), reads the platform's rows, and every
 *                              control produces the action request it claims to.
 *
 * The actions are asserted on the wire (path, body, If-Match), not by their toast: a control
 * that renders but sends nothing fails here. The 412 and 401 paths must be honest.
 *
 * Run: node docs/design/scripts/check-console.cjs
 */
"use strict";

const { chromium } = require("playwright-core");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");

const pack = path.resolve(__dirname, "..");
const shot = (name) => path.join(process.env.CONVEYOR_QA_OUTPUT || os.tmpdir(), name);

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
};

const PROJECT = {
  id: "p-s02",
  key: "S02",
  name: "Skeleton — workspace, tenancy, auth, jobs, SSE",
  description: "",
  status: "active",
  programme_id: "prog-1",
  project_type_code: "delivery",
  phase_code: null,
  owner_user_id: null,
  repo_url: "https://github.com/nonagenticai/conveyor",
  meta: {
    issues: [2, 3],
    plan: "docs/plans/slices/S02-skeleton.md",
    track: "v1",
    expected_outputs: [
      "project-brief",
      "inception-analysis",
      "architecture-design",
    ],
    closure_outputs: ["evidence", "closure-review"],
  },
  version: 1,
};

// A project the seed can leave without a board (an archived row, or a row from before #165).
const NOBOARD = {
  id: "p-noboard",
  key: "X1",
  name: "A project left without a board",
  description: "",
  status: "active",
  programme_id: "prog-1",
  project_type_code: "delivery",
  phase_code: null,
  owner_user_id: null,
  repo_url: null,
  meta: {
    track: "v2",
    measured_status: "proposed",
    tranche: 9,
    expected_outputs: ["project-brief", "inception-analysis", "architecture-design"],
    closure_outputs: ["evidence", "closure-review"],
  },
  version: 1,
};


// The architecture capability's live rows (#385): the tailored profile and its standard, the
// artefacts with their entries and links, the contracts, the board and the two assessment
// baselines. Shaped exactly as the actions return them, so the page is tested against the
// contract and not against a simplified fixture.
const LIVE_ADM = (() => {
  const stamp = "2026-11-16T00:00:00Z";
  const artifacts = [
    {
      id: "art-live-1",
      project_id: "p-s02",
      artifact_type_id: "at-live-1",
      phase_code: "Preliminary",
      plateau: "psa",
      artifact_kind: "principles_catalog",
      title: "Live principles catalog",
      status: "approved",
      artifact_version: 3,
      author_id: "u-1",
      reviewer_id: "u-1",
      reviewed_at: stamp,
      change_initiative: "",
      purpose: "the principles",
      stakeholders: [],
      scope: "tenant",
      document_id: "d-live",
      contract_id: null,
      supersedes_id: null,
      superseded_by_id: null,
      meta: {},
      version: 4,
    },
    {
      id: "art-live-2",
      project_id: "p-s02",
      artifact_type_id: "at-live-2",
      phase_code: "B",
      plateau: "hld",
      artifact_kind: "catalog",
      title: "Live business catalog",
      status: "draft",
      artifact_version: 1,
      author_id: "u-1",
      reviewer_id: null,
      reviewed_at: null,
      change_initiative: "",
      purpose: "the business services",
      stakeholders: [],
      scope: "tenant",
      document_id: null,
      contract_id: "con-live-1",
      supersedes_id: null,
      superseded_by_id: null,
      meta: {},
      version: 1,
    },
  ];
  return {
    profile: {
      id: "prof-live-1",
      key: "software-delivery",
      name: "Live tailored ADM profile",
      phase_set: ["Preliminary", "A", "B"],
      artifact_requirements: { B: { hld: ["at-live-2"] } },
      contract_kinds: { B: "business_contract" },
      status: "adopted",
      adopted_at: stamp,
      board_decision_id: "bd-live-1",
      version: 2,
    },
    standard: [
      {
        id: "at-live-1",
        key: "principles_catalog",
        name: "Principles catalog",
        artifact_kind: "principles_catalog",
        phase_code: "Preliminary",
        plateau: "psa",
        mandatory: true,
        position: 1,
      },
      {
        id: "at-live-2",
        key: "business_service_catalog",
        name: "Business service catalog",
        artifact_kind: "catalog",
        phase_code: "B",
        plateau: "hld",
        mandatory: true,
        position: 2,
      },
    ],
    artifacts,
    entries: {
      "art-live-1": [
        {
          id: "e-live-1",
          artifact_id: "art-live-1",
          axis_a: "P-01",
          axis_b: "",
          value: { statement: "One record per fact" },
          position: 1,
          version: 1,
        },
      ],
      "art-live-2": [
        {
          id: "e-live-2",
          artifact_id: "art-live-2",
          axis_a: "SVC-01",
          axis_b: "",
          value: "Release management",
          position: 1,
          version: 1,
        },
      ],
    },
    links: [
      {
        id: "lk-live-1",
        from_artifact_id: "art-live-1",
        to_artifact_id: "art-live-2",
        kind: "derivation",
        version: 1,
      },
    ],
    contracts: [
      {
        id: "con-live-1",
        project_id: "p-s02",
        kind: "business_contract",
        phase_code: "B",
        title: "Live business contract",
        body: {},
        status: "live",
        approval_id: null,
        approved_by: "u-1",
        approved_at: stamp,
        supersedes_id: null,
        superseded_by_id: null,
        version: 1,
      },
    ],
    boards: [
      {
        id: "board-live-1",
        kind: "architecture_board",
        key: "architecture-board",
        name: "Live Architecture Board",
        charter: "Support the roadmap",
        commissioning_authority: "programme board",
        decision_limits: ["no budget authority"],
        members: ["programme architect"],
        status: "active",
        version: 1,
      },
    ],
    maturity: [
      {
        assessment_id: "mat-live-1",
        stage: "baseline",
        dimensions: [
          { key: "Architecture process", score: 3 },
          { key: "Architecture governance", score: 2 },
        ],
      },
    ],
    skills: [
      {
        assessment_id: "skl-live-1",
        role: "programme architect",
        categories: [{ key: "Architecture skills", score: 4 }],
      },
    ],
  };
})();

// The programme store's rows (#305): programme-scoped benefits, map edges, transitions,
// tranches, reviews, stakeholders and cadence; project-scoped WBS, baselines, register,
// dependencies; and the tenant's in-flight set.
function freshMsp() {
  const stamp = "2026-11-16T00:00:00Z";
  return {
    programme: {
      id: "prog-1",
      name: "conveyor buildout",
      key: "CONV",
      status: "active",
      owner_user_id: null,
      version: 1,
      created_at: stamp,
      updated_at: stamp,
    },
    benefits: [
      {
        id: "ben-1",
        programme_id: "prog-1",
        reference: "BEN-01",
        description: "Deliver units without handoffs",
        category: "delivery",
        objectives: "shorter delivery cycle",
        observable_outcome: "lead time falls",
        kpis: "lead time",
        baseline: 12,
        target: 6,
        measurement_method: "board cycle time",
        capabilities: "delivery management",
        operational_area: "engineering",
        status: "tracking",
        version: 2,
        created_at: stamp,
        updated_at: stamp,
      },
      {
        id: "ben-2",
        programme_id: "prog-1",
        reference: "BEN-02",
        description: "A benefit with no measured transition",
        category: "delivery",
        observable_outcome: "fewer escalations",
        status: "tracking",
        version: 1,
        created_at: stamp,
        updated_at: stamp,
      },
    ],
    links: [
      {
        id: "bl-1",
        programme_id: "prog-1",
        benefit_id: "ben-1",
        output_ref: "T1 plan spine",
        capability_ref: "plan management",
        operational_outcome: "the plan is managed and evidenced",
        strategic_objective: "faster IT delivery",
        kind: "benefit",
        version: 3,
        created_at: stamp,
        updated_at: stamp,
      },
    ],
    transitions: [
      {
        id: "bt-1",
        programme_id: "prog-1",
        benefit_id: "ben-1",
        baseline_measured_at: "2026-10-01T00:00:00Z",
        readiness: "ready",
        enacted_at: "2026-10-15T00:00:00Z",
        post_measured_at: null,
        retired_at: null,
        evidence: "baseline measured in the register",
        version: 3,
      },
    ],
    tranches: [
      {
        id: "tr-1",
        programme_id: "prog-1",
        number: 1,
        entry_criteria: "mandate approved",
        exit_criteria: "baseline frozen",
        baseline_ref: "BL-1",
        review_at: "2026-12-01T00:00:00Z",
        state: "planned",
        version: 2,
      },
      {
        id: "tr-2",
        programme_id: "prog-1",
        number: 2,
        entry_criteria: "tranche 1 reviewed",
        exit_criteria: "benefits measured",
        baseline_ref: "BL-2",
        review_at: "2027-03-01T00:00:00Z",
        state: "planned",
        version: 1,
      },
    ],
    reviews: [
      {
        id: "rv-1",
        programme_id: "prog-1",
        tranche_id: "tr-1",
        tranche_number: 1,
        decision: "continue",
        evidence: { baselines: "frozen", benefit_status: "tracking", risk_register: "no high risk" },
        created_at: "2026-11-01T00:00:00Z",
        version: 1,
      },
    ],
    stakeholders: [
      {
        id: "sp-1",
        programme_id: "prog-1",
        name: "Programme sponsor",
        concerns: "value for money",
        sensitivities: "review timing",
        importance: "high",
        influence: "high",
        interest: "high",
        exposure: "high",
        support_level: "supportive",
        support_trend: "improving",
        engagement_target: "weekly",
        benefits: "BEN-01",
        disbenefits: "",
        version: 2,
      },
      {
        id: "sp-2",
        programme_id: "prog-1",
        name: "Operations lead",
        importance: "medium",
        influence: "medium",
        support_level: "neutral",
        version: 1,
      },
    ],
    cadences: [
      { id: "cd-1", programme_id: "prog-1", kind: "pi", period: "PI 2026.1", state: "active", version: 2 },
      { id: "cd-2", programme_id: "prog-1", kind: "sprint", period: "Sprint 14", state: "planned", version: 1 },
    ],
    wbs: [
      {
        id: "wn-1",
        project_id: "p-s02",
        code: "S02.1",
        kind: "phase",
        title: "Skeleton phase",
        body: "workspace, tenancy, auth",
        acceptance: ["the tenancy gate holds"],
        business_value: 8,
        time_criticality: 5,
        risk_reduction: 5,
        job_size: 5,
        position: 1,
        status: "planned",
        version: 2,
      },
      {
        id: "wn-2",
        project_id: "p-s02",
        code: "S02.2",
        kind: "deliverable",
        title: "Job and SSE slice",
        body: "jobs, events",
        acceptance: ["the event stream resumes"],
        position: 2,
        status: "planned",
        version: 1,
      },
    ],
    baselines: [
      { id: "bl-proj-1", project_id: "p-s02", kind: "scope", taken_at: "2026-11-01T00:00:00Z", version: 1 },
    ],
    registers: [
      {
        id: "ri-1",
        project_id: "p-s02",
        kind: "risk",
        title: "Tenancy leak on the shared board",
        body: "a card could be read by another tenant",
        status: "open",
        severity: "high",
        probability: "low",
        impact: 4,
        version: 2,
      },
      {
        id: "ri-2",
        project_id: "p-s02",
        kind: "assumption",
        title: "One database per service",
        status: "open",
        severity: "medium",
        version: 1,
      },
    ],
    dependencies: [
      { id: "dep-1", project_id: "p-s02", from_project: "p-s02", to_project: "p-noboard", kind: "blocks", status: "open" },
    ],
    inFlight: [
      { id: "p-s02", key: "S02", name: "Skeleton — workspace, tenancy, auth, jobs, SSE", status: "active" },
    ],
  };
}

function freshState() {
  return {
    project: JSON.parse(JSON.stringify(PROJECT)),
    noBoard: JSON.parse(JSON.stringify(NOBOARD)),
    documents: [],
    repositories: [],
    publish: null,
    issues: [
      {
        number: 9,
        title: "The Plan board needs a per-project filter",
        state: "OPEN",
        url: "https://github.com/nonagenticai/conveyor/issues/9",
        labels: [{ name: "feedback" }],
      },
    ],
    cards: [
      {
        id: "c-1",
        board_id: "b-s02",
        list_id: "l-intake",
        card_type_code: "architecture_task",
        name: "T0 — the organisation spine",
        description: "",
        position: 1,
        parent_card_id: null,
        due_at: null,
        due_done_at: null,
        archived_at: null,
        assignee_id: null,
        prev_list_id: null,
        list_changed_at: "2026-11-13T00:00:00Z",
        meta: { issue: 2, unit_key: "S02", process_key: "architecture_task.READY" },
        version: 1,
        created_at: "2026-11-13T00:00:00Z",
        updated_at: "2026-11-13T00:00:00Z",
      },
    ],
    comments: {},
    runs: [
      {
        id: "r-1",
        card_id: "c-1",
        project_id: "p-s02",
        status: "running",
        attempt: 1,
        error_class: null,
        error_message: null,
        started_at: "2026-11-14T00:00:00Z",
        finished_at: null,
        version: 1,
      },
    ],
    approvals: [
      {
        id: "a-1",
        run_id: "r-1",
        card_id: null,
        project_id: "p-s02",
        gate: "merge",
        kind: "process",
        status: "pending",
        required_decisions: 1,
      },
    ],
    // T16 — a converted SOP's draft judge and draft executable, shaped exactly as the actions
    // return them (the page is tested against the contract, not a simplified fixture).
    procedures: [
      {
        id: "proc-1",
        key: "receiving",
        title: "Synthetic receiving SOP",
        description: "Converted from a synthetic SOP.",
        status: "draft",
        version: 1,
        lock_version: 1,
        definition_of_ready: {},
        definition_of_done: {},
        acceptance_criteria: [
          { criterion: "step 1 `verify_stock` executes and its declared effect holds" },
        ],
        inspection: { kind: "checker", rule: "non_empty" },
        outputs: [],
        source_asset_id: "asset-1",
        knowledge_pack_ref: "",
        action_text: "receiving",
        created_at: "2026-11-19T00:00:00Z",
        updated_at: "2026-11-19T00:00:00Z",
      },
    ],
    processes: [
      {
        id: "proc-run-1",
        key: "receiving",
        title: "Synthetic receiving SOP",
        description: "Converted from a synthetic SOP.",
        status: "draft",
        version: 1,
        lock_version: 1,
        card_type_code: null,
        owner_user_id: null,
        outputs: [],
        source_asset_id: "asset-1",
        knowledge_pack_ref: "",
        converter_version: "prose@1",
        approval_id: null,
        steps: [
          {
            position: 1,
            kind: "computed",
            actor: "system",
            action: "procedures.list",
            input_template: {},
            on_result: "fail",
            max_attempts: 1,
            outputs: [],
            action_text: "verify_stock",
            pddl_preconditions: [],
            pddl_effects: { add: ["stock_verified"], delete: [] },
          },
          {
            position: 2,
            kind: "gate",
            actor: "human",
            gate_kind: "approval",
            input_template: {},
            on_result: "fail",
            max_attempts: 1,
            outputs: [],
            action_text: "put_away",
            pddl_preconditions: ["stock_verified"],
            pddl_effects: { add: ["stored"], delete: [] },
          },
        ],
        created_at: "2026-11-19T00:00:00Z",
        updated_at: "2026-11-19T00:00:00Z",
      },
    ],
    procGates: [],
    msp: freshMsp(),
    requests: [],
    listBodies: [],
    planPatches: [],
    createPatches: [],
    failProjects: 0,
    failMove: 0,
    failDocument: 0,
  };
}

// A small real-shaped snapshot for the public-pack shape: two projects, three issues, one board.
// #528: the generated standards record beside the snapshot. One programme artefact (partial), one
// phase artefact (absent, with a published package) and one card artefact (evidenced), so the four
// levels have a row each and the absent/partial states are exercised rather than assumed.
const STANDARDS = {
  schema: "design-standards/v1",
  generated_at: "2026-11-20T12:00:00Z",
  git_sha: "0".repeat(40),
  summary: { artefacts: 3, mandatory: 3, evidenced: 1, partial: 1, absent: 1, findings: 2 },
  by_standard: {
    MSP: { artefacts: 1, evidenced: 0, partial: 1, absent: 0 },
    TOGAF: { artefacts: 1, evidenced: 0, partial: 0, absent: 1 },
    Delivery: { artefacts: 1, evidenced: 1, partial: 0, absent: 0 },
  },
  artefacts: [
    {
      id: "std-msp-vision-delivery",
      standard: "MSP",
      level: "programme",
      subject_kind: "programme",
      category: "vision",
      status: "partial",
      mandatory: true,
      package_key: "std-msp-vision-delivery:programme",
      package_published: true,
      subject: {
        id: "11111111-1111-7111-8111-111111111111",
        key: "11111111-1111-7111-8111-111111111111",
        title: "Delivery programme",
        programme_id: "11111111-1111-7111-8111-111111111111",
      },
      document: { path: "docs/standards/std-msp-vision-delivery.html", sha256: "0".repeat(64) },
    },
    {
      id: "std-togaf-phase-a-dlv",
      standard: "TOGAF",
      level: "project",
      subject_kind: "phase",
      category: "phase-a",
      status: "absent",
      mandatory: true,
      package_key: "std-togaf-phase-a-dlv:DLV",
      package_published: true,
      subject: {
        id: "22222222-2222-7222-8222-222222222222:a",
        key: "DLV/a",
        title: "DLV — A",
        programme_id: "11111111-1111-7111-8111-111111111111",
        project_key: "DLV",
      },
      document: { path: "docs/standards/std-togaf-phase-a-dlv.html", sha256: "0".repeat(64) },
    },
    {
      id: "std-delivery-evidence-card",
      standard: "Delivery",
      level: "work_package",
      subject_kind: "card",
      category: "evidence",
      status: "evidenced",
      mandatory: true,
      package_key: "",
      package_published: true,
      subject: {
        id: "66666666-6666-7666-8666-666666666666",
        key: "66666666-6666-7666-8666-666666666666",
        title: "Evidence card",
        programme_id: "11111111-1111-7111-8111-111111111111",
        project_key: "DLV",
      },
      document: { path: "docs/standards/std-delivery-evidence-card.html", sha256: "0".repeat(64) },
    },
  ],
};

const SNAPSHOT = {
  schema: "design-snapshot/v1",
  generated_at: "2026-11-20T12:00:00Z",
  git_sha: "0".repeat(40),
  payload_sha256: "a".repeat(64),
  tenant: "nonagentic",
  feedback_url: "https://github.com/nonagenticai/conveyor/discussions",
  programme: {
    programme: [
      { id: "prog-1", name: "conveyor buildout", status: "active" },
      // #528: a UUID-shaped programme so the delivery drill route (`#portfolio/<id>`) is
      // exercised against the same id validation the page uses on a real snapshot.
      {
        id: "11111111-1111-7111-8111-111111111111",
        name: "Delivery programme",
        status: "active",
      },
    ],
    projects: [
      {
        id: "p-s02",
        key: "S02",
        name: "Skeleton — workspace, tenancy, auth, jobs, SSE",
        programme_id: "prog-1",
        status: "active",
        meta: {
          track: "v1",
          plan: "docs/plans/slices/S02-skeleton.md",
          issues: [2, 3],
          depends_on: [],
        },
      },
      {
        id: "p-s03",
        key: "S03",
        name: "Domain schema",
        programme_id: "prog-1",
        status: "active",
        meta: {
          track: "v1",
          plan: "docs/plans/slices/S03-domain-schema.md",
          issues: [],
          depends_on: ["S02"],
        },
      },
      {
        id: "p-p0",
        key: "P0",
        name: "Programme definition",
        programme_id: "prog-1",
        status: "active",
        meta: { track: "v1", plan: "", issues: [], depends_on: [] },
      },
      // #528: one project in the UUID programme and one standalone change initiative, both with
      // dated activity and spend, so L1 (programme + standalone) and L2 (drill) are exercised.
      {
        id: "22222222-2222-7222-8222-222222222222",
        key: "DLV",
        name: "Delivery project",
        programme_id: "11111111-1111-7111-8111-111111111111",
        status: "active",
        created_at: "2026-11-13T00:00:00Z",
        updated_at: "2026-11-20T00:00:00Z",
        meta: {
          track: "v1",
          plan: "",
          issues: [],
          depends_on: [],
          tranche: 4,
          plan_start: "2026-11-12",
          plan_end: "2026-11-24",
        },
      },
      {
        id: "33333333-3333-7333-8333-333333333333",
        key: "LONE",
        name: "Standalone initiative",
        programme_id: null,
        status: "active",
        created_at: "2026-11-15T00:00:00Z",
        updated_at: "2026-11-16T00:00:00Z",
        meta: { track: "v1", plan: "", issues: [], depends_on: [] },
      },
      // #528: an in-programme project with spend but no observed event, and a closed project, so
      // the "no dated activity" and "completed" states are both exercised.
      {
        id: "99999999-9999-7999-8999-999999999999",
        key: "QUIET",
        name: "Quiet project",
        programme_id: "11111111-1111-7111-8111-111111111111",
        status: "active",
        created_at: "2026-11-10T00:00:00Z",
        updated_at: "2026-11-11T00:00:00Z",
        meta: { track: "v1", plan: "", issues: [], depends_on: [], tranche: 2 },
      },
      {
        id: "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa",
        key: "DONE",
        name: "Closed project",
        programme_id: "11111111-1111-7111-8111-111111111111",
        status: "closed",
        created_at: "2026-11-01T00:00:00Z",
        updated_at: "2026-11-12T00:00:00Z",
        meta: { track: "v1", plan: "", issues: [], depends_on: [], tranche: 1 },
      },
    ],
    issue_registry: {
      issues: [
        {
          number: 2,
          title: "S02: the skeleton unit",
          state: "OPEN",
          url: "https://github.com/nonagenticai/conveyor/issues/2",
          labels: [{ name: "unit" }, { name: "status:ready" }],
        },
        {
          number: 3,
          title: "S02: a claimed unit",
          state: "OPEN",
          url: "https://github.com/nonagenticai/conveyor/issues/3",
          labels: [{ name: "status:claimed" }],
        },
        {
          number: 4,
          title: "An unlinked ready unit",
          state: "OPEN",
          url: "https://github.com/nonagenticai/conveyor/issues/4",
          labels: [{ name: "status:ready" }],
        },
      ],
    },
  },
  msp: {
    // #528: the MSP artefacts are programme-scoped in the live snapshot; the fixture carries the
    // UUID programme so the L2 rail is exercised against the same route id the page validates.
    // One benefit has a measured baseline (its transition), one honestly has none.
    benefit_profiles: [
      {
        id: "ben-p0",
        reference: "BEN-P0",
        programme_id: "11111111-1111-7111-8111-111111111111",
        description: "A published benefit",
        category: "delivery",
        observable_outcome: "faster delivery",
        target: "half the cycle time",
        baseline: "measured 2026-10-01",
        measurement_method: "cycle time from the run ledger",
        status: "tracking",
        version: 1,
      },
      {
        id: "ben-p1",
        reference: "BEN-P1",
        programme_id: "11111111-1111-7111-8111-111111111111",
        description: "An unmeasured benefit",
        category: "delivery",
        target: "every artifact has a QA version",
        baseline: "not measured - no instrument exists",
        measurement_method: "the artefact ledger",
        status: "draft",
        version: 1,
      },
    ],
    benefit_links: [
      {
        id: "bl-p0",
        benefit_id: "ben-p0",
        programme_id: "11111111-1111-7111-8111-111111111111",
        output_ref: "P0 brief",
        capability_ref: "programme management",
        operational_outcome: "the definition set is published",
        strategic_objective: "trusted programme",
        kind: "benefit",
        version: 1,
      },
    ],
    benefit_transitions: [
      {
        id: "bt-p0",
        benefit_id: "ben-p0",
        programme_id: "11111111-1111-7111-8111-111111111111",
        baseline_measured_at: "2026-10-01T00:00:00Z",
        readiness: "ready",
        version: 1,
      },
    ],
    // #528: when the audit publishes realisation rows they are the benefit rail's truth — a
    // numeric baseline and post measurement for BEN-P0, and an honestly unbaselined BEN-P1 whose
    // baseline is a placeholder with no value.
    benefit_realisation: [
      {
        reference: "BEN-P0",
        programme_id: "11111111-1111-7111-8111-111111111111",
        state: "realised",
        baseline: { value: 12, measured_at: "2026-10-01T00:00:00Z", placeholder: false },
        latest_post: 8,
        delta: 4,
        target: "half the cycle time",
        measurement_method: "cycle time from the run ledger",
        owner_role: "benefit owner",
      },
      {
        reference: "BEN-P1",
        programme_id: "11111111-1111-7111-8111-111111111111",
        state: "unbaselined",
        baseline: { value: null, measured_at: null, placeholder: true },
        latest_post: null,
        delta: null,
      },
    ],
    stakeholder_profiles: [
      {
        id: "sp-p0",
        programme_id: "11111111-1111-7111-8111-111111111111",
        name: "Published sponsor",
        importance: "high",
        influence: "high",
        support_level: "supportive",
        version: 1,
      },
    ],
    tranches: [
      {
        id: "tr-p0",
        programme_id: "11111111-1111-7111-8111-111111111111",
        number: 0,
        state: "closed",
        review_at: "2026-11-17",
        baseline_ref: "BL-P0",
        version: 1,
      },
      {
        id: "tr-p1",
        programme_id: "11111111-1111-7111-8111-111111111111",
        number: 1,
        state: "planned",
        review_at: null,
        baseline_ref: null,
        version: 1,
      },
      {
        id: "tr-p2",
        programme_id: "11111111-1111-7111-8111-111111111111",
        number: 2,
        state: "planned",
        review_at: null,
        baseline_ref: null,
        version: 1,
      },
    ],
    tranche_reviews: [
      {
        id: "rv-p0",
        tranche_id: "tr-p0",
        tranche_number: 0,
        decision: "continue",
        evidence: {
          baselines: "frozen",
          document_url: "https://conveyor.nonagentic.ai/design/docs/output-t0.html",
        },
        reviewed_at: "2026-11-17T00:00:00Z",
        created_at: "2026-11-01T00:00:00Z",
      },
    ],
    wbs_nodes: [
      {
        id: "wn-p0",
        code: "P0.1",
        kind: "phase",
        title: "Published WBS node",
        body: "definition set",
        status: "planned",
        version: 1,
      },
      // #528: the delivery project's own WBS work package — the Plan lane reads it by project id
      // and renders its title, status and WSJF (8 + 5 + 2 over a job size of 5).
      {
        id: "wn-dlv-1",
        project_id: "22222222-2222-7222-8222-222222222222",
        code: "DLV.1",
        kind: "work_package",
        title: "Deliver the pack",
        status: "accepted",
        acceptance: { items: ["a", "b", "c"] },
        job_size: 5,
        business_value: 8,
        time_criticality: 5,
        risk_reduction: 2,
        version: 1,
      },
    ],
    project_baselines: [
      { id: "bl-p0", kind: "scope", taken_at: "2026-11-01T00:00:00Z", version: 1 },
    ],
    register_items: [
      {
        id: "ri-p0",
        kind: "risk",
        title: "Published register risk",
        status: "open",
        severity: "medium",
        version: 1,
      },
    ],
    cadences: [
      { id: "cd-p0", kind: "pi", period: "PI 2026.2", state: "active", version: 1 },
    ],
    project_dependencies: [
      { id: "dep-p0", from_project: "p-p0", to_project: "p-s02", kind: "needs", status: "open" },
    ],
    // #528: the declaration register the dependency rail reads — one typed edge, one untyped, one
    // unknown target and no cycle. The renderer derives the typed/untagged totals from the edges.
    dependencies: {
      edges: [
        { from: "DLV", to: "P0", kind: "needs", typed: true },
        { from: "S03", to: "S02", kind: null, typed: false },
      ],
      counts: { blocks: 0, needs: 1, supersedes: 0, informs: 0 },
      untyped: 1,
      unknown_targets: [{ from: "S02", to: "GONE" }],
      cycles: [],
    },
    // #528: declared state against the review's authorisation. T0 is authorised by a continue
    // review; T1 has no review at all and carries the review_missing flag.
    tranche_reconciliation: [
      {
        number: 0,
        declared_state: "closed",
        authorised: true,
        review: { decision: "continue", decided_at: "2026-11-17T00:00:00Z" },
        projects: { total: 1, active: 0, closed: 1, archived: 0 },
        flags: [],
      },
      {
        number: 1,
        declared_state: "planned",
        authorised: false,
        review: null,
        projects: { total: 0, active: 0, closed: 0, archived: 0 },
        flags: ["review_missing"],
      },
    ],
  },
  work: {
    boards: [
      { id: "b-s02", project_id: "p-s02" },
      {
        id: "44444444-4444-7444-8444-444444444444",
        project_id: "22222222-2222-7222-8222-222222222222",
        name: "Delivery board",
        position: 0,
      },
      // #528: a standalone initiative is a portfolio row only when it is active/paused AND has a
      // board or a card. LONE carries a board, so the boundary rule is exercised, not bypassed.
      {
        id: "b-lone",
        project_id: "33333333-3333-7333-8333-333333333333",
        name: "Standalone board",
        position: 0,
      },
    ],
    lists: [
      { id: "l-done", board_id: "b-s02", kind: "done" },
      {
        id: "55555555-5555-7555-8555-555555555551",
        board_id: "44444444-4444-7444-8444-444444444444",
        name: "Active",
        kind: "active",
        position: 1,
      },
      {
        id: "55555555-5555-7555-8555-555555555552",
        board_id: "44444444-4444-7444-8444-444444444444",
        name: "Done",
        kind: "done",
        position: 2,
      },
    ],
    cards: [
      { id: "c-1", board_id: "b-s02", list_id: "l-done" },
      // #528: a work package with two children (one done), and one loose card.
      {
        id: "66666666-6666-7666-8666-666666666666",
        board_id: "44444444-4444-7444-8444-444444444444",
        list_id: "55555555-5555-7555-8555-555555555551",
        card_type_id: "ct-delivery",
        name: "Work package Alpha",
        parent_card_id: null,
        created_at: "2026-11-13T00:00:00Z",
        list_changed_at: "2026-11-14T00:00:00Z",
        archived_at: null,
      },
      {
        id: "77777777-7777-7777-8777-777777777771",
        board_id: "44444444-4444-7444-8444-444444444444",
        list_id: "55555555-5555-7555-8555-555555555551",
        card_type_id: "ct-delivery",
        name: "Child one",
        parent_card_id: "66666666-6666-7666-8666-666666666666",
        created_at: "2026-11-14T00:00:00Z",
        list_changed_at: "2026-11-15T00:00:00Z",
        archived_at: null,
      },
      {
        id: "77777777-7777-7777-8777-777777777772",
        board_id: "44444444-4444-7444-8444-444444444444",
        list_id: "55555555-5555-7555-8555-555555555552",
        card_type_id: "ct-delivery",
        name: "Child two",
        parent_card_id: "66666666-6666-7666-8666-666666666666",
        created_at: "2026-11-15T00:00:00Z",
        list_changed_at: "2026-11-16T00:00:00Z",
        archived_at: null,
      },
      // #528: an archived child that is still in an active lane. It must render once, in the
      // Archived column — never also in its old lane (and the tile must count it as done).
      {
        id: "77777777-7777-7777-8777-777777777773",
        board_id: "44444444-4444-7444-8444-444444444444",
        list_id: "55555555-5555-7555-8555-555555555551",
        card_type_id: "ct-delivery",
        name: "Child three",
        parent_card_id: "66666666-6666-7666-8666-666666666666",
        created_at: "2026-11-12T00:00:00Z",
        list_changed_at: "2026-11-13T00:00:00Z",
        archived_at: "2026-11-18T00:00:00Z",
      },
      {
        id: "88888888-8888-7888-8888-888888888888",
        board_id: "44444444-4444-7444-8444-444444444444",
        list_id: "55555555-5555-7555-8555-555555555551",
        card_type_id: "ct-delivery",
        name: "Loose card",
        parent_card_id: null,
        created_at: "2026-11-17T00:00:00Z",
        list_changed_at: "2026-11-17T00:00:00Z",
        archived_at: null,
        // #528: the card the audit's P0 rework package materialised as. Its key matches the
        // package, so L3's Rework lane renders the tile and L4 leads with the register panel.
        rework_key: "contract_missing:tranche-4",
        rule: "contract_missing",
        severity: "P0",
      },
      // #528: an archived root card — a closed project's completed work stays visible.
      {
        id: "bbbbbbbb-bbbb-7bbb-8bbb-bbbbbbbbbbbb",
        board_id: "44444444-4444-7444-8444-444444444444",
        list_id: "55555555-5555-7555-8555-555555555551",
        card_type_id: "ct-delivery",
        name: "Archived card",
        parent_card_id: null,
        created_at: "2026-11-09T00:00:00Z",
        list_changed_at: "2026-11-10T00:00:00Z",
        archived_at: "2026-11-11T00:00:00Z",
      },
    ],
    card_types: [
      {
        id: "ct-delivery",
        code: "delivery_task",
        label: "Delivery task",
        definition: {
          acceptance: ["READY: the card is scoped", "DONE: the evidence is attached"],
        },
      },
    ],
    project_activity: [
      {
        project_id: "22222222-2222-7222-8222-222222222222",
        started_at: "2026-11-13T00:00:00Z",
        last_activity_at: "2026-11-20T00:00:00Z",
      },
      {
        project_id: "33333333-3333-7333-8333-333333333333",
        started_at: "2026-11-15T00:00:00Z",
        last_activity_at: "2026-11-16T00:00:00Z",
      },
      {
        project_id: "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa",
        started_at: "2026-11-01T00:00:00Z",
        last_activity_at: "2026-11-12T00:00:00Z",
      },
      // No window for the QUIET project: the page must say "no dated activity", never invent one.
    ],
    runs: [
      {
        id: "run-dlv-1",
        project_id: "22222222-2222-7222-8222-222222222222",
        card_id: "66666666-6666-7666-8666-666666666666",
        status: "succeeded",
        created_at: "2026-11-19T00:00:00Z",
        started_at: "2026-11-19T00:01:00Z",
        finished_at: "2026-11-19T00:05:00Z",
      },
      {
        id: "run-dlv-2",
        project_id: "22222222-2222-7222-8222-222222222222",
        card_id: "77777777-7777-7777-8777-777777777771",
        status: "failed",
        created_at: "2026-11-20T00:00:00Z",
        started_at: "2026-11-20T00:01:00Z",
        finished_at: "2026-11-20T00:02:00Z",
      },
    ],
    run_steps: [
      { id: "rs-dlv-1", run_id: "run-dlv-1", status: "succeeded", created_at: "2026-11-19T00:01:00Z" },
      { id: "rs-dlv-2", run_id: "run-dlv-1", status: "succeeded", created_at: "2026-11-19T00:02:00Z" },
      { id: "rs-dlv-3", run_id: "run-dlv-2", status: "failed", created_at: "2026-11-20T00:01:00Z" },
    ],
    approvals: [
      {
        id: "ap-dlv-1",
        card_id: "66666666-6666-7666-8666-666666666666",
        project_id: "22222222-2222-7222-8222-222222222222",
        gate: "delivery.review",
        kind: "gate",
        status: "approved",
        required_decisions: 2,
        requested_by: "u-1",
        expires_at: null,
        run_id: null,
        created_at: "2026-11-20T00:00:00Z",
        updated_at: "2026-11-20T00:00:00Z",
        version: 1,
      },
    ],
    approval_decisions: [],
  },
  // T16: the published procedure shape the signed-out page renders.
  config: {
    procedures: [
      {
        id: "proc-1",
        key: "receiving",
        title: "Synthetic receiving SOP",
        status: "published",
        version: 2,
      },
    ],
    processes: [
      {
        id: "proc-run-1",
        key: "receiving",
        title: "Synthetic receiving SOP",
        status: "published",
        version: 2,
      },
    ],
    process_steps: [
      {
        id: "ps-1",
        process_id: "proc-run-1",
        position: 1,
        kind: "computed",
        actor: "system",
        action: "procedures.list",
      },
    ],
  },
  organisation: {},
  artefacts: {
    documents: [
      {
        id: "doc-p0-vision",
        project_id: "p-p0",
        card_id: null,
        kind: "document",
        title: "Vision and mandate",
        category: "vision",
        unit: "P0",
        updated_at: "2026-11-14T00:00:00Z",
        created_at: "2026-11-14T00:00:00Z",
      },
    ],
  },
  // The published architecture capability (#385): the adopted profile, the standard, the
  // artefacts with their entries, the links, the contracts, the board and the two baselines.
  architecture: {
    profiles: [
      {
        id: "prof-pub-1",
        key: "software-delivery",
        name: "Published tailored ADM profile",
        phase_set: ["Preliminary", "A", "B"],
        artifact_requirements: { B: { hld: ["at-pub-2"] } },
        contract_kinds: { B: "business_contract" },
        status: "adopted",
        adopted_at: "2026-11-12T00:00:00Z",
        board_decision_id: "bd-pub-1",
        version: 1,
      },
    ],
    standard: [
      {
        id: "at-pub-1",
        key: "principles_catalog",
        name: "Principles catalog",
        artifact_kind: "principles_catalog",
        phase_code: "Preliminary",
        plateau: "psa",
        mandatory: true,
        position: 1,
      },
      {
        id: "at-pub-2",
        key: "business_service_catalog",
        name: "Business service catalog",
        artifact_kind: "catalog",
        phase_code: "B",
        plateau: "hld",
        mandatory: true,
        position: 2,
      },
    ],
    artifacts: [
      {
        id: "art-pub-1",
        project_id: "p-p0",
        artifact_type_id: "at-pub-1",
        phase_code: "Preliminary",
        plateau: "psa",
        artifact_kind: "principles_catalog",
        title: "Published principles catalog",
        status: "approved",
        artifact_version: 3,
        change_initiative: "",
        purpose: "the principles",
        stakeholders: [],
        scope: "tenant",
        document_id: "doc-p0-vision",
        contract_id: null,
        meta: {},
        version: 4,
        entries: [
          {
            id: "e-pub-1",
            artifact_id: "art-pub-1",
            axis_a: "P-01",
            axis_b: "",
            value: { statement: "One record per fact" },
            position: 1,
            version: 1,
          },
        ],
      },
      {
        id: "art-pub-2",
        project_id: "p-p0",
        artifact_type_id: "at-pub-2",
        phase_code: "B",
        plateau: "hld",
        artifact_kind: "catalog",
        title: "Published business catalog",
        status: "approved",
        artifact_version: 2,
        change_initiative: "",
        purpose: "the business services",
        stakeholders: [],
        scope: "tenant",
        document_id: null,
        contract_id: "con-pub-1",
        meta: {},
        version: 3,
        entries: [
          {
            id: "e-pub-2",
            artifact_id: "art-pub-2",
            axis_a: "SVC-01",
            axis_b: "",
            value: "Release management",
            position: 1,
            version: 1,
          },
        ],
      },
    ],
    links: [
      {
        id: "lk-pub-1",
        from_artifact_id: "art-pub-1",
        to_artifact_id: "art-pub-2",
        kind: "derivation",
        version: 1,
      },
    ],
    contracts: [
      {
        id: "con-pub-1",
        project_id: "p-p0",
        kind: "business_contract",
        phase_code: "B",
        title: "Published business contract",
        body: {},
        status: "live",
        approval_id: null,
        approved_by: "u-1",
        approved_at: "2026-11-12T00:00:00Z",
        supersedes_id: null,
        superseded_by_id: null,
        version: 1,
      },
      // #528: the delivery project's own TOGAF rows, so L3 shows the contract and the Phase G
      // compliance level rather than "no architecture contract".
      {
        id: "con-dlv-1",
        project_id: "22222222-2222-7222-8222-222222222222",
        kind: "delivery_contract",
        phase_code: "F",
        title: "Delivery contract",
        body: {},
        status: "live",
        approval_id: null,
        approved_by: "u-1",
        approved_at: "2026-11-13T00:00:00Z",
        supersedes_id: null,
        superseded_by_id: null,
        version: 1,
      },
    ],
    compliance_reviews: [
      {
        id: "cr-dlv-1",
        project_id: "22222222-2222-7222-8222-222222222222",
        contract_id: "con-dlv-1",
        level: "compliant",
        outcome: "design reviewed",
        reviewed_at: "2026-11-18T00:00:00Z",
        reviewed_by: "u-1",
        version: 1,
      },
    ],
    waivers: [],
    board: [
      {
        id: "board-pub-1",
        kind: "architecture_board",
        key: "architecture-board",
        name: "Published Architecture Board",
        charter: "Support the roadmap",
        commissioning_authority: "programme board",
        decision_limits: ["no budget authority"],
        members: ["programme architect"],
        status: "active",
        version: 1,
      },
    ],
    maturity: [
      {
        assessment_id: "mat-pub-1",
        stage: "baseline",
        dimension: "Architecture process",
        score: 2,
        assessed_at: "2026-11-10T00:00:00Z",
        evidence: "review",
      },
      {
        assessment_id: "mat-pub-1",
        stage: "baseline",
        dimension: "Architecture governance",
        score: 1,
        assessed_at: "2026-11-10T00:00:00Z",
        evidence: "review",
      },
    ],
    skills: [
      {
        assessment_id: "skl-pub-1",
        role: "programme architect",
        job_key: null,
        category: "Architecture skills",
        proficiency: 3,
        assessed_at: "2026-11-10T00:00:00Z",
        evidence: "assessment",
      },
    ],
    // #528: the delivery project's TOGAF A–H progress — phase A evidenced by a compliant contract,
    // B–H not evidenced, and B named as the next required phase. The phase rail reads it by id.
    phase_progress: [
      {
        project: "22222222-2222-7222-8222-222222222222",
        declared: { adm_phase: "A", plateau: "hld" },
        phases: [
          {
            phase: "a",
            state: "evidenced",
            contract: "delivery_contract",
            compliance: "compliant",
          },
          { phase: "b", state: "not-evidenced" },
          { phase: "c", state: "not-evidenced" },
          { phase: "d", state: "not-evidenced" },
          { phase: "e", state: "not-evidenced" },
          { phase: "f", state: "not-evidenced" },
          { phase: "g", state: "not-evidenced" },
          { phase: "h", state: "not-evidenced" },
        ],
        next_required: "b",
      },
    ],
  },
  // #434: the T7 incident record as the snapshot's `ops` section. One card, its runbook
  // selection resolved to a published procedure title and its linked postmortem asset.
  ops: {
    incidents: [
      {
        id: "inc-1",
        board_id: "b-s02",
        name: "Incident: checkout-api (sev2)",
        service: "checkout-api",
        severity: "sev2",
        detected_at: "2026-11-14T01:00:00Z",
        triaged_at: "2026-11-14T01:01:00Z",
        mitigated_at: "2026-11-14T01:02:00Z",
        resolved_at: "2026-11-14T01:03:00Z",
        runbook_procedure_id: "proc-1",
        runbook_key: "crash-loop",
        runbook_title: "Crash-loop runbook",
        postmortem_asset_id: "asset-1",
        postmortem_title: "Postmortem: crash loop",
      },
    ],
  },
  // #528: the delivery view's spend. `cost_usd` is a decimal string, as the ledger report renders
  // it; an unpriced call is counted, never shown as "$0".
  measure: {
    inference: {
      by_project: [
        {
          project_id: "22222222-2222-7222-8222-222222222222",
          calls: 3,
          prompt_tokens: 1_400_000,
          completion_tokens: 100_000,
          total_tokens: 1_500_000,
          cost_usd: "0.420000",
          priced_calls: 2,
          unpriced_calls: 1,
        },
        {
          project_id: "33333333-3333-7333-8333-333333333333",
          calls: 1,
          prompt_tokens: 250_000,
          completion_tokens: 0,
          total_tokens: 250_000,
          cost_usd: "0.035000",
          priced_calls: 1,
          unpriced_calls: 0,
        },
        // Unpriced only: the row must render "—*", never "$0.00".
        {
          project_id: "99999999-9999-7999-8999-999999999999",
          calls: 1,
          prompt_tokens: 40_000,
          completion_tokens: 2_000,
          total_tokens: 42_000,
          cost_usd: "0.000000",
          priced_calls: 0,
          unpriced_calls: 1,
        },
      ],
      by_card: [
        {
          card_id: "66666666-6666-7666-8666-666666666666",
          calls: 1,
          prompt_tokens: 900_000,
          completion_tokens: 60_000,
          total_tokens: 960_000,
          cost_usd: "0.270000",
          priced_calls: 1,
          unpriced_calls: 0,
        },
        {
          card_id: "77777777-7777-7777-8777-777777777771",
          calls: 2,
          prompt_tokens: 500_000,
          completion_tokens: 40_000,
          total_tokens: 540_000,
          cost_usd: "0.150000",
          priced_calls: 1,
          unpriced_calls: 1,
        },
      ],
    },
  },
  // #528: the conformance audit's rework register as the snapshot publishes it. Three packages:
  // one P0 applied to the delivery project with a card, one P1 with no card (not materialised),
  // and one suppressed P1. The scorecard's open counts come from the packages, not its own row.
  rework: {
    schema: "conveyor-rework/v1",
    generated_at: "2026-11-20T11:00:00Z",
    scorecard: {
      MSP: {
        rules: 12,
        rules_clean: 4,
        rules_with_gaps: 8,
        gaps: 88,
        p0: 22,
        p1: 65,
        p2: 1,
        suppressed: 0,
        packages: 3,
        findings: 1,
      },
      TOGAF: {
        rules: 10,
        rules_clean: 7,
        rules_with_gaps: 3,
        gaps: 12,
        p0: 0,
        p1: 8,
        p2: 4,
        suppressed: 0,
        packages: 0,
        findings: 0,
      },
      SAFe: {
        rules: 8,
        rules_clean: 6,
        rules_with_gaps: 2,
        gaps: 5,
        p0: 0,
        p1: 4,
        p2: 1,
        suppressed: 0,
        packages: 0,
        findings: 0,
      },
      PMP: {
        rules: 9,
        rules_clean: 5,
        rules_with_gaps: 4,
        gaps: 14,
        p0: 1,
        p1: 6,
        p2: 2,
        suppressed: 0,
        packages: 0,
        findings: 0,
      },
      SPEC: {
        rules: 11,
        rules_clean: 8,
        rules_with_gaps: 3,
        gaps: 9,
        p0: 0,
        p1: 3,
        p2: 3,
        suppressed: 0,
        packages: 0,
        findings: 0,
      },
    },
    summary: {
      packages: 3,
      open: 2,
      suppressed: 1,
      subjects: 4,
      duplicates: 0,
      resolved: 1,
      by_standard: { MSP: 3 },
      by_severity: { P0: 1, P1: 1 },
      by_status: { open: 2, suppressed: 1 },
    },
    packages: [
      {
        key: "contract_missing:tranche-4",
        standard: "MSP",
        spec: "MSP §8.3",
        rule: "contract_missing",
        severity: "P0",
        status: "open",
        title: "Publish the tranche-4 delivery contract",
        apply_to: "DLV",
        owner_project: "T9",
        artefact: "architecture contract",
        scope: "tranche 4",
        depends_on: [],
        subjects: [
          {
            subject: "conveyor:DLV",
            detail: "the delivery project has no evidenced A-phase contract",
            severity: "P0",
            disposition: "open",
            owner: "architecture board",
            evidence: { path: "docs/design/data/architecture.json" },
          },
        ],
        dod: [{ item: "the tranche-4 delivery contract is published", met: false }],
        acceptance_command:
          "python3 scripts/audit-programme-conformance.py --snapshot docs/design/data --report",
      },
      {
        key: "wbs_absent:tranche-4",
        standard: "MSP",
        spec: "MSP §8.1",
        rule: "wbs_absent",
        severity: "P1",
        status: "open",
        title: "Publish the tranche-4 WBS",
        apply_to: "DLV",
        evidence: { path: "docs/design/data/msp.json" },
      },
      {
        key: "benefit_unbaselined:tranche-4",
        standard: "MSP",
        spec: "MSP §8.3",
        rule: "benefit_unbaselined",
        severity: "P1",
        status: "suppressed",
        title: "Baseline the tranche-4 benefits",
        apply_to: "DLV",
      },
    ],
  },
};

// #528: delivery.js's dependencyRail is handed the whole MSP section (`D.dependencyRail(data.msp)`)
// while the published snapshot nests the register at `msp.dependencies`; expose the register at
// both levels so the rail reads the same rows the snapshot publishes.
function json(res, status, body, headers = {}) {  res.writeHead(status, Object.assign({ "Content-Type": "application/json; charset=utf-8" }, headers));
  res.end(JSON.stringify(body));
}

function boardView() {
  return {
    board: { id: "b-s02", project_id: "p-s02", name: "Board", version: 1 },
    lists: [
      { list: { id: "l-intake", board_id: "b-s02", name: "Intake", kind: "intake", position: 1, color: null, wip_limit: null, version: 1 }, card_count: 0 },
      { list: { id: "l-active", board_id: "b-s02", name: "Active", kind: "active", position: 2, color: null, wip_limit: null, version: 1 }, card_count: 0 },
      { list: { id: "l-done", board_id: "b-s02", name: "Done", kind: "done", position: 3, color: null, wip_limit: null, version: 1 }, card_count: 0 },
    ],
    labels: [],
    fields: [],
    members: [],
  };
}

function runDetail(state, id) {
  const run = state.runs.find((row) => row.id === id);
  return {
    run,
    steps: run
      ? [
          {
            id: "s-1",
            position: 1,
            attempt: 1,
            action: "cards.get",
            status: "succeeded",
            input: { id: run.card_id },
            output: { pr_url: "https://github.com/nonagenticai/conveyor/pull/12" },
            started_at: "2026-11-14T00:00:01Z",
            finished_at: "2026-11-14T00:00:02Z",
          },
        ]
      : [],
    approvals: state.approvals.filter((row) => row.run_id === id),
    process: { id: "pr-1", title: "architecture_task.READY", version: 1 },
    tree: { parent_run_id: null, step_id: null, depth: 0, children: [] },
    inputs: {},
  };
}

function startServer({ config, state, snapshot, outputs, standards }) {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, "http://127.0.0.1");
    const pathname = decodeURIComponent(url.pathname);
    if (pathname === "/config.json") {
      if (!config) return json(res, 404, { title: "Not Found" });
      return json(res, 200, config);
    }
    if (pathname === "/data/issues.json") {
      // A feedback issue the panel can intake into a project.
      return json(res, 200, { issues: state.issues });
    }
    if (pathname.startsWith("/api/")) {
      state.requests.push({ method: req.method, path: `${pathname}${url.search}`, headers: req.headers });
      if (pathname === "/api/me" && req.method === "GET") {
        return json(res, 200, {
          principal: { id: "u-1", subject: "test:operator", display_name: "Test Operator", email: null },
          active_tenant: null,
          memberships: [{ tenant_id: "t-1", slug: "nonagentic", name: "Non-agentic AI", role: "admin" }],
          is_super_admin: true,
        });
      }
      if (pathname === "/api/actions" && req.method === "GET") {
        const names = [
          "architecture.profile_get",
          "architecture.artifacts_list",
          "architecture.artifacts_entries",
          "architecture.artifacts_links",
          "architecture.contracts_list",
          "architecture.board_list",
          "architecture.maturity_list",
          "architecture.skills_list",
          "cards.create",
          "cards.move",
          "cards.comment",
          "cards.claim",
          "cards.release",
          "card_types.list",
          "catalogues.list",
          "approvals.decide",
          "approvals.list",
          "approvals.request",
          "procedures.list",
          "procedures.get",
          "procedures.ingest_sop",
          "procedures.ingest_process",
          "procedures.ingest_rdf",
          "procedures.validate",
          "procedures.review_source",
          "procedures.publish",
          "processes.list",
          "processes.get",
          "processes.publish",
          "process_runs.start",
          "programmes.list",
          "programmes.refresh",
          "programmes.update",
          "programmes.refresh",
          "benefits.list",
          "benefits.create",
          "benefits.update",
          "benefits.delete",
          "benefits.link",
          "benefits.link_list",
          "benefits.unlink",
          "benefits.transition_list",
          "benefits.transition_upsert",
          "tranches.list",
          "tranches.create",
          "tranches.update",
          "tranches.delete",
          "tranches.review_list",
          "tranches.review",
          "stakeholders.profile_list",
          "stakeholders.profile_create",
          "stakeholders.profile_update",
          "stakeholders.profile_delete",
          "cadences.list",
          "cadences.create",
          "cadences.update",
          "cadences.delete",
          "wbs.list",
          "wbs.create",
          "wbs.update",
          "wbs.delete",
          "baselines.list",
          "baselines.take",
          "registers.list",
          "registers.create",
          "registers.update",
          "registers.delete",
          "project_dependencies.list",
          "project_dependencies.create",
          "project_dependencies.delete",
          "portfolio.in_flight",
          "runs.cancel",
          "runs.get",
          "runs.list",
          "documents.compose",
          "documents.get",
          "documents.list",
          "documents.update",
          "documents.download",
          "documents.delete",
          "documents.create",
          "documents.publish",
          "documents.request_publish",
          "documents.preview",
          "documents.return",
          "documents.revision_list",
          "documents.revision_get",
          "documents.revision_restore",
          "projects.activate",
          "projects.archive",
          "projects.close",
          "projects.create",
          "projects.pause",
          "projects.programme_meta",
          "projects.update",
          "publish.start",
          "publish.status",
          "repositories.create",
          "repositories.list",
          "repositories.update",
          "secrets.create",
        ];
        return json(res, 200, {
          actions: names.map((name) => ({
            name,
            version: 1,
            kind: "internal",
            spec_hash: "0".repeat(64),
            scopes: ["cards:write"],
            idempotent: false,
            timeout_ms: 30000,
          })),
          generated_at: "2026-11-13T00:00:00Z",
        });
      }
      if (pathname === "/api/projects" && req.method === "GET") {
        if (state.failProjects) return json(res, state.failProjects, { title: "Unauthorized", detail: "authentication required" });
        return json(res, 200, { items: [state.project, state.noBoard], next_cursor: null });
      }
      if (pathname === "/api/projects/p-s02" && req.method === "GET") {
        return json(res, 200, { project: state.project, boards: [{ id: "b-s02", project_id: "p-s02", name: "Board", version: 1 }] });
      }
      if (pathname === "/api/projects/p-noboard" && req.method === "GET") {
        // No board: a real state, not a read failure.
        return json(res, 200, { project: state.noBoard, boards: [] });
      }
      if (pathname === "/api/boards/b-s02" && req.method === "GET") {
        return json(res, 200, boardView());
      }
      if (pathname === "/api/boards/b-s02/cards" && req.method === "GET") {
        return json(res, 200, { items: state.cards, next_cursor: null });
      }
      if (pathname.startsWith("/api/cards/") && req.method === "GET") {
        const id = pathname.split("/")[3];
        const card = state.cards.find((row) => row.id === id);
        if (!card) return json(res, 404, { title: "Not Found", detail: "no such card" });
        return json(res, 200, {
          card,
          labels: [],
          comments: state.comments[id] || [],
          task_lists: [],
          custom_field_values: [],
        });
      }
      if (pathname === "/api/actions/card_types.list/invocations") {
        // The read `catalogues.list` cannot serve: card_type is not one of the eleven
        // catalogue namespaces. The definitions carry the outputs the card table renders.
        return json(res, 200, {
          items: [
            {
              id: "ct-1",
              code: "architecture_task",
              label: "Architecture task",
              is_active: true,
              definition: {
                outputs: ["draft_artifact_id", "reviewed_artifact_id"],
                acceptance: [],
              },
            },
            {
              id: "ct-2",
              code: "bdd_test_development",
              label: "BDD test development",
              is_active: true,
              definition: { outputs: ["pr_url", "head_sha", "merge_sha"], acceptance: [] },
            },
          ],
          next_cursor: null,
        });
      }
      if (pathname === "/api/actions/architecture.profile_get/invocations") {
        return readBody(req, () =>
          json(res, 200, { profile: LIVE_ADM.profile, standard: LIVE_ADM.standard }),
        );
      }
      if (pathname === "/api/actions/architecture.artifacts_list/invocations") {
        return readBody(req, () =>
          json(res, 200, { items: LIVE_ADM.artifacts, next_cursor: null }),
        );
      }
      if (pathname === "/api/actions/architecture.artifacts_entries/invocations") {
        return readBody(req, (body) => {
          assert.ok(body.input && body.input.id, "artifacts_entries names an artefact");
          json(res, 200, { items: LIVE_ADM.entries[body.input.id] || [], next_cursor: null });
        });
      }
      if (pathname === "/api/actions/architecture.artifacts_links/invocations") {
        return readBody(req, (body) => {
          assert.ok(body.input && body.input.id, "artifacts_links names an artefact");
          const id = body.input.id;
          json(res, 200, {
            items: LIVE_ADM.links.filter(
              (row) => row.from_artifact_id === id || row.to_artifact_id === id,
            ),
            next_cursor: null,
          });
        });
      }
      if (pathname === "/api/actions/architecture.contracts_list/invocations") {
        return readBody(req, () =>
          json(res, 200, { items: LIVE_ADM.contracts, next_cursor: null }),
        );
      }
      if (pathname === "/api/actions/architecture.board_list/invocations") {
        return readBody(req, (body) => {
          assert.equal(
            body.input && body.input.kind,
            "architecture_board",
            "board_list asks for the architecture board",
          );
          json(res, 200, {
            items: LIVE_ADM.boards.filter((row) => row.kind === body.input.kind),
            next_cursor: null,
          });
        });
      }
      if (pathname === "/api/actions/architecture.maturity_list/invocations") {
        return readBody(req, () =>
          json(res, 200, { items: LIVE_ADM.maturity, next_cursor: null }),
        );
      }
      if (pathname === "/api/actions/architecture.skills_list/invocations") {
        return readBody(req, () =>
          json(res, 200, { items: LIVE_ADM.skills, next_cursor: null }),
        );
      }
      if (pathname === "/api/actions/procedures.list/invocations") {
        return json(res, 200, { items: state.procedures, next_cursor: null });
      }
      if (pathname === "/api/actions/processes.list/invocations") {
        return json(res, 200, { items: state.processes, next_cursor: null });
      }
      if (pathname === "/api/actions/processes.get/invocations") {
        return readBody(req, (body) => {
          const found = state.processes.find((row) => row.id === body.input.process_id);
          assert.ok(found, "processes.get names a real process");
          json(res, 200, found);
        });
        return;
      }
      if (pathname === "/api/actions/procedures.get/invocations") {
        return readBody(req, (body) => {
          const found = state.procedures.find((row) => row.id === body.input.procedure_id);
          assert.ok(found, "procedures.get names a real procedure");
          json(res, 200, found);
        });
        return;
      }
      if (pathname === "/api/actions/procedures.ingest_sop/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.title, "Synthetic receiving SOP", "ingest_sop sends the title");
          assert.equal(input.process_key, "receiving-stub", "ingest_sop sends the process key");
          assert.equal(input.source_key, "receiving-sop-stub", "ingest_sop sends the source key");
          assert.equal(input.source.kind, "text", "a pasted SOP is a text source");
          assert.match(String(input.source.text || ""), /Receive stock/i, "the SOP body travels");
          const procedure = {
            ...state.procedures[0],
            id: "proc-new",
            key: input.process_key,
            title: input.title,
          };
          const process = {
            ...state.processes[0],
            id: "run-new",
            key: input.process_key,
            title: input.title,
          };
          state.procedures.push(procedure);
          state.processes.push(process);
          json(res, 201, {
            source_asset_id: "asset-1",
            procedure_id: procedure.id,
            process_id: process.id,
            steps: 2,
            converter_version: "prose@1",
            knowledge_pack_ref: "",
          });
        });
        return;
      }
      if (pathname === "/api/actions/procedures.validate/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.process_id, "run-new", "validate names the draft it was given");
          json(res, 200, {
            valid: true,
            checks: ["the PDDL literals parse", "every step names a published judge"],
          });
        });
        return;
      }
      if (pathname === "/api/actions/procedures.review_source/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.asset_id, "asset-1", "the review names the draft's source asset");
          assert.equal(body.input.decision, "approve", "the approve control sends approve");
          assert.ok(body.input.note, "the review carries the operator's note");
          json(res, 200, { asset_id: body.input.asset_id, status: "approved" });
        });
        return;
      }
      if (pathname === "/api/actions/approvals.request/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.ok(
            ["processes.publish", "procedures.publish"].includes(input.gate),
            "the request names a publication gate",
          );
          assert.ok(input.target, "the request names its target");
          // The real service opens a PENDING gate; a second person decides it.
          const approval = {
            id: `gate-${state.procGates.length + 1}`,
            gate: input.gate,
            kind: input.kind,
            status: "pending",
            target: input.target,
            approvals: 0,
            required_decisions: 1,
            requested_by: "u-2",
          };
          state.procGates.push(approval);
          json(res, 200, approval);
        });
        return;
      }
      if (pathname === "/api/actions/approvals.list/invocations") {
        return readBody(req, (body) => {
          const status = (body.input || {}).status;
          json(res, 200, {
            items: state.procGates.filter((row) => row.status === status),
            next_cursor: null,
          });
        });
        return;
      }
      if (pathname === "/api/actions/processes.publish/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.process_id, "run-new", "the process publish names the draft");
          assert.equal(
            body.approval_id,
            "gate-1",
            "the approval id rides the CALL body, never the input",
          );
          json(res, 200, { id: body.input.process_id, status: "published", version: 2 });
        });
        return;
      }
      if (pathname === "/api/actions/procedures.publish/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.procedure_id, "proc-new", "the judge publish names the draft judge");
          assert.equal(
            body.approval_id,
            "gate-2",
            "the judge publish carries its own approved gate",
          );
          json(res, 200, { id: body.input.procedure_id, status: "published", version: 2 });
        });
        return;
      }
      if (pathname === "/api/actions/cards.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.board_id, "b-s02", "cards.create sends the board");
          assert.equal(input.list_id, "l-intake", "cards.create sends the intake lane");
          assert.ok(
            ["3", "9"].includes(String(input.meta && input.meta.issue)),
            "cards.create carries the issue link",
          );
          assert.equal(input.card_type_code, "architecture_task", "cards.create sends the card type");
          assert.equal(
            input.meta && input.meta.process_key,
            "architecture_task.READY",
            "cards.create binds the card type's READY process",
          );
          const id = `c-${state.cards.length + 1}`;
          state.cards.push({
            id,
            board_id: "b-s02",
            list_id: input.list_id,
            card_type_code: input.card_type_code,
            name: input.name,
            description: "",
            position: state.cards.length + 1,
            parent_card_id: null,
            due_at: null,
            due_done_at: null,
            archived_at: null,
            assignee_id: null,
            prev_list_id: null,
            list_changed_at: "2026-11-13T00:00:00Z",
            meta: input.meta,
            version: 1,
            created_at: "2026-11-13T00:00:00Z",
            updated_at: "2026-11-13T00:00:00Z",
          });
          json(res, 201, { id, name: input.name, version: 1 });
        });
        return;
      }
      if (pathname === "/api/actions/cards.move/invocations") {
        return readBody(req, (body) => {
          if (state.failMove) return json(res, state.failMove, { title: "Precondition Failed", detail: "cards.move: the card moved" });
          const card = state.cards.find((row) => row.id === body.input.id);
          assert.ok(card, "cards.move names a real card");
          assert.equal(req.headers["if-match"], `"${card.version}"`, "cards.move carries If-Match");
          card.list_id = body.input.list_id;
          card.version += 1;
          json(res, 200, card);
        });
        return;
      }
      if (pathname === "/api/actions/cards.comment/invocations") {
        return readBody(req, (body) => {
          const id = body.input.id;
          const comment = {
            id: `k-${(state.comments[id] || []).length + 1}`,
            card_id: id,
            author_id: "u-1",
            author_name: "Test Operator",
            body: body.input.body,
            edited_at: null,
            version: 1,
            created_at: "2026-11-13T00:00:00Z",
            updated_at: "2026-11-13T00:00:00Z",
          };
          state.comments[id] = (state.comments[id] || []).concat(comment);
          json(res, 201, comment);
        });
        return;
      }
      if (pathname === "/api/actions/cards.claim/invocations") {
        return readBody(req, (body) => {
          const card = state.cards.find((row) => row.id === body.input.card_id);
          card.assignee_id = "u-1";
          card.version += 1;
          json(res, 200, card);
        });
        return;
      }
      if (pathname === "/api/actions/cards.release/invocations") {
        return readBody(req, (body) => {
          const card = state.cards.find((row) => row.id === body.input.card_id);
          card.assignee_id = null;
          card.version += 1;
          json(res, 200, card);
        });
        return;
      }
      if (pathname === "/api/actions/programmes.list/invocations") {
        return readBody(req, () => {
          json(res, 200, {
            items: [
              {
                id: "prog-1",
                name: "conveyor buildout",
                status: "active",
                owner_user_id: null,
                version: 1,
                created_at: "2026-11-12T00:00:00Z",
                updated_at: "2026-11-13T00:00:00Z",
              },
            ],
            next_cursor: null,
          });
        });
      }
      /* ── #305: the programme store — the reads the definition and plan pages issue ─── */
      if (pathname === "/api/actions/benefits.list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.programme_id, "prog-1", "benefits.list is programme-scoped");
          json(res, 200, { items: state.msp.benefits, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/benefits.link_list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.programme_id, "prog-1", "benefits.link_list is programme-scoped");
          json(res, 200, { items: state.msp.links, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/benefits.transition_list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.programme_id, "prog-1", "benefits.transition_list is programme-scoped");
          json(res, 200, { items: state.msp.transitions, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/tranches.list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.programme_id, "prog-1", "tranches.list is programme-scoped");
          json(res, 200, { items: state.msp.tranches, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/tranches.review_list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.programme_id, "prog-1", "tranches.review_list is programme-scoped");
          json(res, 200, { items: state.msp.reviews, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/stakeholders.profile_list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.programme_id, "prog-1", "stakeholders.profile_list is programme-scoped");
          json(res, 200, { items: state.msp.stakeholders, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/cadences.list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.programme_id, "prog-1", "cadences.list is programme-scoped");
          json(res, 200, { items: state.msp.cadences, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/wbs.list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.project_id, "p-s02", "wbs.list is project-scoped");
          json(res, 200, { items: state.msp.wbs, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/baselines.list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.project_id, "p-s02", "baselines.list is project-scoped");
          json(res, 200, { items: state.msp.baselines, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/registers.list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.project_id, "p-s02", "registers.list is project-scoped");
          json(res, 200, { items: state.msp.registers, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/project_dependencies.list/invocations") {
        return readBody(req, (body) => {
          assert.equal(body.input.project_id, "p-s02", "project_dependencies.list is project-scoped");
          json(res, 200, { items: state.msp.dependencies, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/portfolio.in_flight/invocations") {
        return readBody(req, () => json(res, 200, { items: state.msp.inFlight, next_cursor: null }));
      }

      /* ── #305: the programme store — the writes the dialogs issue ──────────────────── */
      if (pathname === "/api/actions/programmes.refresh/invocations") {
        return readBody(req, () => {
          // The image's embedded seed applied to the tenant: every row created on a fresh tenant.
          json(res, 200, { created: 49, updated: 0, skipped: 0 });
        });
      }
      if (pathname === "/api/actions/programmes.update/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          const row = state.msp.programme;
          assert.equal(input.id, row.id, "programmes.update names the programme row");
          assert.equal(req.headers["if-match"], `"${row.version}"`, "programmes.update carries If-Match");
          if (input.name !== undefined) row.name = input.name;
          if (input.status !== undefined) row.status = input.status;
          row.version += 1;
          json(res, 200, row);
        });
      }
      // #517: the programme refresh - the console's button calls this action and toasts its
      // counts. The stub answers the same shape the live action does (created/updated/skipped);
      // 49 is the row count the case asserts in the toast.
      if (pathname === "/api/actions/programmes.refresh/invocations") {
        return readBody(req, () => {
          json(res, 200, { created: 49, updated: 0, skipped: 0 });
        });
      }
      if (pathname === "/api/actions/benefits.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.programme_id, "prog-1", "benefits.create carries its parent programme");
          const row = Object.assign({}, input, {
            id: `ben-${state.msp.benefits.length + 1}`,
            version: 1,
            created_at: "2026-11-16T00:00:00Z",
            updated_at: "2026-11-16T00:00:00Z",
          });
          state.msp.benefits.push(row);
          json(res, 201, row);
        });
      }
      if (pathname === "/api/actions/benefits.update/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          const row = state.msp.benefits.find((x) => x.id === input.id);
          assert.ok(row, "benefits.update names a real benefit");
          assert.equal(req.headers["if-match"], `"${row.version}"`, "benefits.update carries If-Match");
          Object.assign(row, input);
          row.version += 1;
          json(res, 200, row);
        });
      }
      if (pathname === "/api/actions/benefits.delete/invocations") {
        return readBody(req, (body) => {
          const index = state.msp.benefits.findIndex((x) => x.id === body.input.id);
          assert.ok(index >= 0, "benefits.delete names a real benefit");
          assert.equal(req.headers["if-match"], `"${state.msp.benefits[index].version}"`, "benefits.delete carries If-Match");
          state.msp.benefits.splice(index, 1);
          json(res, 200, { deleted: true });
        });
      }
      if (pathname === "/api/actions/benefits.link/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.programme_id, "prog-1", "benefits.link carries its parent programme");
          for (const key of ["benefit_id", "output_ref", "capability_ref", "operational_outcome", "strategic_objective"]) {
            assert.ok(input[key], `benefits.link names the ${key} level`);
          }
          const row = Object.assign({}, input, { id: `bl-${state.msp.links.length + 1}`, version: 1 });
          state.msp.links.push(row);
          json(res, 201, row);
        });
      }
      if (pathname === "/api/actions/benefits.unlink/invocations") {
        return readBody(req, (body) => {
          const index = state.msp.links.findIndex((x) => x.id === body.input.id);
          assert.ok(index >= 0, "benefits.unlink names a real edge");
          assert.equal(req.headers["if-match"], `"${state.msp.links[index].version}"`, "benefits.unlink carries If-Match");
          state.msp.links.splice(index, 1);
          json(res, 200, { deleted: true });
        });
      }
      if (pathname === "/api/actions/benefits.transition_upsert/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.ok(input.benefit_id, "benefits.transition_upsert names its benefit");
          assert.ok(input.baseline_measured_at, "benefits.transition_upsert carries the measured baseline");
          assert.ok(input.post_measured_at, "benefits.transition_upsert carries the post-transition measurement");
          const existing = state.msp.transitions.find((x) => x.benefit_id === input.benefit_id);
          if (existing) {
            assert.equal(req.headers["if-match"], `"${existing.version}"`, "a transition edit carries If-Match");
            Object.assign(existing, input);
            existing.version += 1;
            return json(res, 200, existing);
          }
          const row = Object.assign({}, input, { id: `bt-${state.msp.transitions.length + 1}`, version: 1 });
          state.msp.transitions.push(row);
          json(res, 201, row);
        });
      }
      if (pathname === "/api/actions/tranches.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.programme_id, "prog-1", "tranches.create carries its parent programme");
          const row = Object.assign({}, input, {
            id: `tr-${state.msp.tranches.length + 1}`,
            state: input.state || "planned",
            version: 1,
          });
          state.msp.tranches.push(row);
          json(res, 201, row);
        });
      }
      if (pathname === "/api/actions/tranches.update/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          const row = state.msp.tranches.find((x) => x.id === input.id);
          assert.ok(row, "tranches.update names a real tranche");
          assert.equal(req.headers["if-match"], `"${row.version}"`, "tranches.update carries If-Match");
          Object.assign(row, input);
          row.version += 1;
          json(res, 200, row);
        });
      }
      if (pathname === "/api/actions/tranches.delete/invocations") {
        return readBody(req, (body) => {
          const index = state.msp.tranches.findIndex((x) => x.id === body.input.id);
          assert.ok(index >= 0, "tranches.delete names a real tranche");
          assert.equal(req.headers["if-match"], `"${state.msp.tranches[index].version}"`, "tranches.delete carries If-Match");
          state.msp.tranches.splice(index, 1);
          json(res, 200, { deleted: true });
        });
      }
      if (pathname === "/api/actions/tranches.review/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          const tranche = state.msp.tranches.find((x) => x.id === input.id);
          assert.ok(tranche, "tranches.review names a real tranche");
          assert.ok(["continue", "pause", "stop"].includes(input.decision), "tranches.review records a decision");
          const evidence = input.evidence || {};
          for (const key of ["baselines", "benefit_status", "risk_register"]) {
            assert.ok(evidence[key], `tranches.review carries the ${key} evidence`);
          }
          const review = {
            id: `rv-${state.msp.reviews.length + 1}`,
            tranche_id: tranche.id,
            tranche_number: tranche.number,
            decision: input.decision,
            evidence,
            created_at: "2026-11-16T00:00:00Z",
            version: 1,
          };
          state.msp.reviews.push(review);
          tranche.state = "reviewed";
          json(res, 201, review);
        });
      }
      if (pathname === "/api/actions/stakeholders.profile_create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.programme_id, "prog-1", "stakeholders.profile_create carries its parent programme");
          const row = Object.assign({}, input, { id: `sp-${state.msp.stakeholders.length + 1}`, version: 1 });
          state.msp.stakeholders.push(row);
          json(res, 201, row);
        });
      }
      if (pathname === "/api/actions/stakeholders.profile_update/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          const row = state.msp.stakeholders.find((x) => x.id === input.id);
          assert.ok(row, "stakeholders.profile_update names a real profile");
          assert.equal(req.headers["if-match"], `"${row.version}"`, "stakeholders.profile_update carries If-Match");
          Object.assign(row, input);
          row.version += 1;
          json(res, 200, row);
        });
      }
      if (pathname === "/api/actions/stakeholders.profile_delete/invocations") {
        return readBody(req, (body) => {
          const index = state.msp.stakeholders.findIndex((x) => x.id === body.input.id);
          assert.ok(index >= 0, "stakeholders.profile_delete names a real profile");
          assert.equal(req.headers["if-match"], `"${state.msp.stakeholders[index].version}"`, "stakeholders.profile_delete carries If-Match");
          state.msp.stakeholders.splice(index, 1);
          json(res, 200, { deleted: true });
        });
      }
      if (pathname === "/api/actions/cadences.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.programme_id, "prog-1", "cadences.create carries its parent programme");
          const row = Object.assign({}, input, { id: `cd-${state.msp.cadences.length + 1}`, version: 1 });
          state.msp.cadences.push(row);
          json(res, 201, row);
        });
      }
      if (pathname === "/api/actions/cadences.update/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          const row = state.msp.cadences.find((x) => x.id === input.id);
          assert.ok(row, "cadences.update names a real cadence");
          assert.equal(req.headers["if-match"], `"${row.version}"`, "cadences.update carries If-Match");
          Object.assign(row, input);
          row.version += 1;
          json(res, 200, row);
        });
      }
      if (pathname === "/api/actions/cadences.delete/invocations") {
        return readBody(req, (body) => {
          const index = state.msp.cadences.findIndex((x) => x.id === body.input.id);
          assert.ok(index >= 0, "cadences.delete names a real cadence");
          assert.equal(req.headers["if-match"], `"${state.msp.cadences[index].version}"`, "cadences.delete carries If-Match");
          state.msp.cadences.splice(index, 1);
          json(res, 200, { deleted: true });
        });
      }
      if (pathname === "/api/actions/wbs.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.project_id, "p-s02", "wbs.create carries its parent project");
          const row = Object.assign({}, input, { id: `wn-${state.msp.wbs.length + 1}`, version: 1 });
          state.msp.wbs.push(row);
          json(res, 201, row);
        });
      }
      if (pathname === "/api/actions/wbs.update/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          const row = state.msp.wbs.find((x) => x.id === input.id);
          assert.ok(row, "wbs.update names a real node");
          assert.equal(req.headers["if-match"], `"${row.version}"`, "wbs.update carries If-Match");
          Object.assign(row, input);
          row.version += 1;
          json(res, 200, row);
        });
      }
      if (pathname === "/api/actions/wbs.delete/invocations") {
        return readBody(req, (body) => {
          const index = state.msp.wbs.findIndex((x) => x.id === body.input.id);
          assert.ok(index >= 0, "wbs.delete names a real node");
          assert.equal(req.headers["if-match"], `"${state.msp.wbs[index].version}"`, "wbs.delete carries If-Match");
          state.msp.wbs.splice(index, 1);
          json(res, 200, { deleted: true });
        });
      }
      if (pathname === "/api/actions/registers.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.project_id, "p-s02", "registers.create carries its parent project");
          const row = Object.assign({}, input, { id: `ri-${state.msp.registers.length + 1}`, version: 1 });
          state.msp.registers.push(row);
          json(res, 201, row);
        });
      }
      if (pathname === "/api/actions/registers.update/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          const row = state.msp.registers.find((x) => x.id === input.id);
          assert.ok(row, "registers.update names a real register row");
          assert.equal(req.headers["if-match"], `"${row.version}"`, "registers.update carries If-Match");
          Object.assign(row, input);
          row.version += 1;
          json(res, 200, row);
        });
      }
      if (pathname === "/api/actions/registers.delete/invocations") {
        return readBody(req, (body) => {
          const index = state.msp.registers.findIndex((x) => x.id === body.input.id);
          assert.ok(index >= 0, "registers.delete names a real register row");
          assert.equal(req.headers["if-match"], `"${state.msp.registers[index].version}"`, "registers.delete carries If-Match");
          state.msp.registers.splice(index, 1);
          json(res, 200, { deleted: true });
        });
      }
      if (pathname === "/api/actions/project_dependencies.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.project_id, "p-s02", "project_dependencies.create carries its parent project");
          assert.ok(input.to_project, "project_dependencies.create names the target project");
          assert.ok(["blocks", "needs", "supersedes", "informs"].includes(input.kind), "project_dependencies.create names a typed edge");
          const row = {
            id: `dep-${state.msp.dependencies.length + 1}`,
            project_id: input.project_id,
            from_project: input.project_id,
            to_project: input.to_project,
            kind: input.kind,
            status: "open",
          };
          state.msp.dependencies.push(row);
          json(res, 201, row);
        });
      }
      if (pathname === "/api/actions/project_dependencies.delete/invocations") {
        return readBody(req, (body) => {
          const index = state.msp.dependencies.findIndex((x) => x.id === body.input.id);
          assert.ok(index >= 0, "project_dependencies.delete names a real edge");
          state.msp.dependencies.splice(index, 1);
          json(res, 200, { deleted: true });
        });
      }
      if (pathname === "/api/actions/baselines.take/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.project_id, "p-s02", "baselines.take carries its parent project");
          assert.ok(["scope", "schedule", "cost"].includes(input.kind), "baselines.take names the baseline kind");
          const row = {
            id: `bl-${state.msp.baselines.length + 1}-${input.kind}`,
            project_id: input.project_id,
            kind: input.kind,
            taken_at: "2026-11-16T00:00:00Z",
            version: 1,
          };
          state.msp.baselines.push(row);
          json(res, 201, row);
        });
      }
      if (pathname === "/api/actions/runs.list/invocations") {
        return readBody(req, () => {
          json(res, 200, { items: state.runs, next_cursor: null });
        });
      }
      if (pathname === "/api/actions/runs.get/invocations") {
        return readBody(req, (body) => {
          const detail = runDetail(state, body.input.id);
          if (!detail.run) return json(res, 404, { title: "Not Found", detail: "no such run" });
          json(res, 200, detail);
        });
      }
      if (pathname === "/api/actions/runs.cancel/invocations") {
        return readBody(req, (body) => {
          const run = state.runs.find((row) => row.id === body.input.id);
          assert.ok(run, "runs.cancel names a real run");
          assert.ok(body.input.reason, "runs.cancel carries a reason");
          run.status = "cancelled";
          run.version += 1;
          run.finished_at = "2026-11-14T00:01:00Z";
          json(res, 200, run);
        });
      }
      if (pathname === "/api/actions/approvals.decide/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          const approval =
            state.approvals.find((row) => row.id === input.approval_id) ||
            state.procGates.find((row) => row.id === input.approval_id);
          assert.ok(approval, "approvals.decide names a real gate");
          assert.ok(["approve", "reject"].includes(input.decision), "a valid decision");
          assert.ok(input.note, "the decision carries a note");
          approval.status = input.decision === "approve" ? "approved" : "rejected";
          if (approval.status === "approved") approval.approvals = 1;
          json(res, 200, approval);
        });
      }
      if (pathname === "/api/actions/process_runs.start/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          const card = state.cards.find((row) => row.id === input.card_id);
          assert.ok(card, "process_runs.start names a real card");
          assert.equal(
            input.process_key,
            (card.meta || {}).process_key,
            "process_runs.start sends the card's process",
          );
          const run = {
            id: `r-${state.runs.length + 1}`,
            card_id: card.id,
            project_id: "p-s02",
            status: "running",
            attempt: 1,
            error_class: null,
            error_message: null,
            started_at: "2026-11-14T00:02:00Z",
            finished_at: null,
            version: 1,
          };
          state.runs.push(run);
          json(res, 200, run);
        });
      }
      if (pathname === "/api/actions/projects.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.ok(input.title, "projects.create sends a title");
          const project = {
            id: "p-new",
            key: "NEW",
            name: input.title,
            description: input.description || "",
            status: "active",
            programme_id: input.programme_id || null,
            project_type_code: null,
            phase_code: null,
            owner_user_id: null,
            repo_url: null,
            meta: {},
            version: 1,
          };
          json(res, 201, { project });
        });
        return;
      }
      if (pathname === "/api/actions/projects.programme_meta/invocations") {
        return readBody(req, (body) => {
          try {
            const input = body.input || {};
            const target =
              input.id === "p-s02"
                ? state.project
                : input.id === "p-noboard"
                  ? state.noBoard
                  : null;
            if (!target) {
              // The create-time class/plan write: the row was just created at version 1.
              assert.equal(
                req.headers["if-match"],
                '"1"',
                "the create-time programme write is versioned",
              );
              state.planPatches.push(input.meta && input.meta.plan);
              state.createPatches.push(input.meta || {});
              return json(res, 200, { project: { id: input.id, version: 2 } });
            }
            assert.equal(
              req.headers["if-match"],
              `"${target.version}"`,
              "projects.programme_meta carries If-Match",
            );
            assert.ok(input.meta && typeof input.meta === "object", "programme_meta sends an object");
            target.meta = Object.assign({}, target.meta, input.meta);
            target.version += 1;
            json(res, 200, {
              project: target,
              boards:
                input.id === "p-s02"
                  ? [{ id: "b-s02", project_id: "p-s02", name: "Board", version: 1 }]
                  : [],
            });
          } catch (error) {
            console.error("stub projects.programme_meta:", error);
            json(res, 500, { title: "Internal", detail: String(error) });
          }
        });
        return;
      }
      if (pathname === "/api/actions/projects.update/invocations") {
        return readBody(req, (body) => {
          try {
            const input = body.input || {};
            const target =
              input.id === "p-s02"
                ? state.project
                : input.id === "p-noboard"
                  ? state.noBoard
                  : null;
            if (!target) {
              // The create-time class/plan patch: `projects.create` has no meta, so the console
              // follows it with one update. No If-Match — the row was just created.
              assert.equal(req.headers["if-match"], undefined, "the create-time patch has no If-Match");
              state.planPatches.push(input.meta && input.meta.plan);
              state.createPatches.push(input.meta || {});
              return json(res, 200, { project: { id: input.id, version: 2 } });
            }
            assert.equal(req.headers["if-match"], `"${target.version}"`, "projects.update carries If-Match");
            for (const key of ["name", "description", "status", "repo_url"]) {
              if (input[key] !== undefined) target[key] = input[key];
            }
            if (input.meta) target.meta = Object.assign({}, target.meta, input.meta);
            target.version += 1;
            json(res, 200, {
              project: target,
              boards:
                input.id === "p-s02"
                  ? [{ id: "b-s02", project_id: "p-s02", name: "Board", version: 1 }]
                  : [],
            });
          } catch (error) {
            console.error("stub projects.update:", error);
            json(res, 500, { title: "Internal", detail: String(error) });
          }
        });
        return;
      }
      if (pathname === "/api/actions/documents.list/invocations") {
        return readBody(req, (body) => {
          const scoped = body.input.project_id;
          const parent = body.input.parent_document_id;
          assert.ok(
            scoped === undefined || ["p-s02", "p-noboard"].includes(scoped),
            "documents.list is project-scoped or the cross-project register",
          );
          if (parent === undefined) {
            state.listBodies.push(scoped === undefined ? "register" : scoped);
          }
          const requested = body.input.publish_requested;
          const items = state.documents
            .filter((doc) => (parent === undefined ? doc.kind !== "attachment" : doc.parent_document_id === parent))
            .filter((doc) => (scoped === undefined ? true : doc.project_id === scoped))
            .filter((doc) => (requested === undefined ? true : !!doc.publish_requested_at))
            .map((doc) => {
              const summary = Object.assign({}, doc);
              delete summary.body;
              return summary;
            });
          json(res, 200, { output: { items, next_cursor: null } });
        });
        return;
      }
      if (pathname === "/api/actions/documents.compose/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.project_id, "p-s02", "documents.compose is scoped to the project");
          assert.ok(input.title && input.body, "documents.compose sends a title and a body");
          state.lastCompose = input;
          const doc = {
            id: `d-${state.documents.length + 1}`,
            project_id: input.project_id,
            card_id: input.card_id || null,
            parent_document_id: null,
            kind: "document",
            title: input.title,
            status: "active",
            content_type: "text/markdown",
            size_bytes: input.body.length,
            content_hash: "a".repeat(64),
            body: input.body,
            external_ref: null,
            meta: input.meta || {},
            category: input.category || null,
            unit: input.unit || null,
            created_by: "u-1",
            created_by_name: "Test Operator",
            updated_by: "u-1",
            updated_by_name: "Test Operator",
            published_at: null,
            published_by: null,
            published_by_name: null,
            publish_requested_at: null,
            publishable: !!input.publishable,
            version: 1,
            created_at: "2026-11-13T00:00:00Z",
            updated_at: "2026-11-13T00:00:00Z",
          };
          state.documents.unshift(doc);
          json(res, 201, doc);
        });
        return;
      }
      if (pathname === "/api/actions/documents.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.ok(input.parent_document_id, "an attachment names its parent output");
          assert.ok(input.content_base64, "an attachment carries its bytes");
          const attachment = {
            id: `a-${state.documents.length + 1}`,
            project_id: "p-s02",
            card_id: null,
            parent_document_id: input.parent_document_id,
            kind: "attachment",
            title: input.title,
            status: "active",
            content_type: input.content_type,
            size_bytes: 8,
            content_hash: "b".repeat(64),
            body: null,
            external_ref: null,
            meta: { content_type: input.content_type, size_bytes: 8 },
            category: null,
            unit: null,
            created_by: "u-1",
            created_by_name: "Test Operator",
            publishable: false,
            version: 1,
            created_at: "2026-11-13T00:00:00Z",
            updated_at: "2026-11-13T00:00:00Z",
          };
          state.documents.push(attachment);
          json(res, 201, attachment);
        });
        return;
      }
      if (pathname === "/api/actions/documents.get/invocations") {
        return readBody(req, (body) => {
          const doc = state.documents.find((row) => row.id === body.input.id);
          if (!doc) return json(res, 404, { title: "Not Found", detail: "no such document" });
          json(res, 200, doc);
        });
        return;
      }
      if (pathname === "/api/actions/documents.publish/invocations") {
        return readBody(req, (body) => {
          if (state.failPublishIncomplete && body.input.publish) {
            state.failPublishIncomplete = false;
            return json(res, 409, {
              title: "Conflict",
              code: "output_incomplete",
              detail: "the output is missing required sections: Boundary, Who it serves",
              errors: [
                { field: "body", code: "output_incomplete", detail: "missing required section: Boundary" },
                { field: "body", code: "output_incomplete", detail: "missing required section: Who it serves" },
              ],
            });
          }
          const doc = state.documents.find((row) => row.id === body.input.id);
          assert.ok(doc, "documents.publish names a real document");
          assert.equal(req.headers["if-match"], `"${doc.version}"`, "documents.publish carries If-Match");
          doc.publishable = !!body.input.publish;
          doc.published_at = doc.publishable ? "2026-11-13T00:00:00Z" : null;
          doc.published_by = doc.publishable ? "u-1" : null;
          doc.published_by_name = doc.publishable ? "Test Operator" : null;
          doc.publish_requested_at = null;
          json(res, 200, doc);
        });
        return;
      }
      if (pathname === "/api/actions/documents.request_publish/invocations") {
        return readBody(req, (body) => {
          const doc = state.documents.find((row) => row.id === body.input.id);
          assert.ok(doc, "documents.request_publish names a real document");
          assert.equal(
            req.headers["if-match"],
            `"${doc.version}"`,
            "documents.request_publish carries If-Match",
          );
          doc.publish_requested_at = "2026-11-13T00:00:00Z";
          json(res, 200, doc);
        });
        return;
      }
      if (pathname === "/api/actions/documents.return/invocations") {
        return readBody(req, (body) => {
          const doc = state.documents.find((row) => row.id === body.input.id);
          assert.ok(doc, "documents.return names a real document");
          assert.equal(req.headers["if-match"], `"${doc.version}"`, "documents.return carries If-Match");
          assert.ok(
            typeof body.input.note === "string" && body.input.note.trim(),
            "documents.return carries the note",
          );
          if (!doc.publish_requested_at) {
            return json(res, 409, {
              title: "Conflict",
              code: "not_requested",
              detail: "no publish request is open on this output",
            });
          }
          doc.publish_requested_at = null;
          doc.meta = Object.assign({}, doc.meta || {}, {
            rework: {
              note: body.input.note,
              missing: ["Boundary", "Who it serves"],
              by: "u-1",
              at: "2026-11-17T00:00:00Z",
              of_version: doc.version,
            },
          });
          json(res, 200, doc);
        });
        return;
      }
      if (pathname === "/api/actions/documents.preview/invocations") {
        return readBody(req, (body) => {
          const doc = state.documents.find((row) => row.id === body.input.id);
          assert.ok(doc, "documents.preview names a real document");
          json(res, 200, {
            output: {
              html: `<h1>${doc.title}</h1><p>preview body</p>`,
              category: doc.category,
              missing: ["Boundary", "Who it serves"],
            },
          });
        });
        return;
      }
      if (pathname === "/api/actions/documents.revision_list/invocations") {
        return readBody(req, (body) => {
          assert.ok(body.input.id, "documents.revision_list names its document");
          json(res, 200, [
            {
              id: "r-1",
              seq: 1,
              label: "created",
              content_hash: "c".repeat(64),
              author_id: "u-1",
              author: "Test Operator",
              status: "draft",
              created_at: "2026-11-13T00:00:00Z",
            },
          ]);
        });
        return;
      }
      if (pathname === "/api/actions/documents.revision_get/invocations") {
        return readBody(req, (body) => {
          const doc = state.documents.find((row) => row.id === body.input.id);
          json(res, 200, {
            id: "r-1",
            seq: body.input.seq,
            label: "created",
            content_hash: "c".repeat(64),
            author_id: "u-1",
            author: "Test Operator",
            status: "draft",
            created_at: "2026-11-13T00:00:00Z",
            content: { body: doc ? doc.body : "", title: doc ? doc.title : "" },
          });
        });
        return;
      }
      if (pathname === "/api/actions/documents.revision_restore/invocations") {
        return readBody(req, (body) => {
          const doc = state.documents.find((row) => row.id === body.input.id);
          assert.ok(doc, "documents.revision_restore names a real document");
          assert.equal(
            req.headers["if-match"],
            `"${doc.version}"`,
            "documents.revision_restore carries If-Match",
          );
          doc.version += 1;
          doc.publishable = false;
          json(res, 200, doc);
        });
        return;
      }
      if (pathname === "/api/actions/documents.update/invocations") {
        return readBody(req, (body) => {
          if (state.failDocument) {
            return json(res, state.failDocument, {
              title: "Precondition Failed",
              detail: "documents.update: the row moved",
            });
          }
          const doc = state.documents.find((row) => row.id === body.input.id);
          assert.ok(doc, "documents.update names a real document");
          assert.equal(req.headers["if-match"], `"${doc.version}"`, "documents.update carries If-Match");
          if (body.input.title !== undefined) doc.title = body.input.title;
          if (body.input.body !== undefined) {
            doc.body = body.input.body;
            doc.size_bytes = body.input.body.length;
          }
          if (body.input.category !== undefined) doc.category = body.input.category || null;
          if (body.input.unit !== undefined) doc.unit = body.input.unit || null;
          doc.version += 1;
          json(res, 200, doc);
        });
        return;
      }
      if (pathname === "/api/actions/documents.download/invocations") {
        return readBody(req, (body) => {
          const doc = state.documents.find((row) => row.id === body.input.id);
          if (!doc) return json(res, 404, { title: "Not Found", detail: "no such document" });
          const bytes = Buffer.from(doc.body || "", "utf8");
          json(res, 200, {
            id: doc.id,
            content_type: "text/markdown",
            content_base64: bytes.toString("base64"),
            content_hash: "a".repeat(64),
            byte_len: bytes.length,
          });
        });
      }
      if (pathname === "/api/actions/documents.delete/invocations") {
        return readBody(req, (body) => {
          const index = state.documents.findIndex((row) => row.id === body.input.id);
          assert.ok(index >= 0, "documents.delete names a real document");
          assert.equal(
            req.headers["if-match"],
            `"${state.documents[index].version}"`,
            "documents.delete carries If-Match",
          );
          state.documents.splice(index, 1);
          json(res, 200, { deleted: true });
        });
      }
      if (pathname === "/api/actions/repositories.list/invocations") {
        return readBody(req, () => json(res, 200, { items: state.repositories }));
      }
      if (pathname === "/api/actions/secrets.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.ok(input.name && input.value, "secrets.create sends a name and a value");
          json(res, 201, {
            id: `s-${Date.now()}`,
            name: input.name,
            scope: input.scope || "tenant",
            key_id: "k-1",
            version: 1,
            created_at: "2026-11-13T00:00:00Z",
          });
        });
        return;
      }
      if (pathname === "/api/actions/repositories.create/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.ok(input.url, "repositories.create sends a URL");
          const repo = {
            id: `r-${state.repositories.length + 1}`,
            provider: "github",
            url: input.url,
            default_branch: input.default_branch || "main",
            installation_ref: null,
            credential_secret_id: input.credential_secret_id || null,
            version: 1,
            created_at: "2026-11-13T00:00:00Z",
            updated_at: "2026-11-13T00:00:00Z",
          };
          state.repositories.push(repo);
          json(res, 201, repo);
        });
        return;
      }
      if (pathname === "/api/actions/repositories.update/invocations") {
        return readBody(req, (body) => {
          const repo = state.repositories.find((row) => row.id === body.input.id);
          assert.ok(repo, "repositories.update names a real repository");
          assert.equal(req.headers["if-match"], `"${repo.version}"`, "repositories.update carries If-Match");
          if (body.input.credential_secret_id !== undefined) {
            repo.credential_secret_id = body.input.credential_secret_id;
          }
          repo.version += 1;
          json(res, 200, repo);
        });
        return;
      }
      if (pathname === "/api/actions/publish.start/invocations") {
        return readBody(req, (body) => {
          const repo = state.repositories.find((row) => row.id === body.input.repository_id);
          assert.ok(repo, "publish.start names a real repository");
          assert.ok(repo.credential_secret_id, "publish.start targets a repository with a token");
          state.publish = { state: "pending", job_id: "j-1", pr_url: null };
          json(res, 200, { job_id: "j-1", state: "pending", already_running: false });
        });
        return;
      }
      if (pathname === "/api/actions/publish.status/invocations") {
        return readBody(req, (body) => {
          if (!state.publish) {
            return json(res, 200, {
              state: "none",
              job_id: null,
              pr_url: null,
              error: null,
              generated_at: null,
              snapshot_sha256: null,
              outputs: null,
            });
          }
          state.publish.state = "succeeded";
          state.publish.pr_url = "https://github.com/nonagenticai/nonagentic_homepage/pull/7";
          state.publish.generated_at = "2026-11-13T00:00:00Z";
          state.publish.snapshot_sha256 = "b".repeat(64);
          state.publish.outputs = 1;
          json(res, 200, state.publish);
        });
        return;
      }
      return json(res, 404, { title: "Not Found", detail: `no stub route for ${pathname}` });
    }
    if (pathname === "/data/snapshot.js") {
      res.writeHead(200, { "Content-Type": MIME[".js"] });
      if (snapshot) {
        return res.end("window.CONVEYOR_SNAPSHOT = " + JSON.stringify(snapshot) + ";");
      }
      // Every fixture that is not the explicit with-snapshot shape sees the placeholder. On a
      // publish branch the pack itself carries a generated, live payload; letting the static
      // file answer made the no-snapshot fixtures load it (#400: the WBS tab; #459: the
      // signed-out "No published snapshot is loaded" and the 401 fallback).
      return res.end("window.CONVEYOR_SNAPSHOT = null;");
    }
    // #386's fixture serves its own published-output index; the committed docs/outputs.js is a
    // placeholder (null), so the public queue would otherwise have no docs to count.
    if (pathname === "/data/standards.js") {
      res.writeHead(200, { "Content-Type": MIME[".js"] });
      return res.end(
        standards
          ? "window.CONVEYOR_STANDARDS = " + JSON.stringify(standards) + ";"
          : "window.CONVEYOR_STANDARDS = null;",
      );
    }
    // #386's fixture serves its own published-output index; the committed docs/outputs.js is a
    // placeholder (null), so the public queue would otherwise have no docs to count.
    if (pathname === "/docs/outputs.js" && outputs) {
      res.writeHead(200, { "Content-Type": MIME[".js"] });
      return res.end("window.CONVEYOR_OUTPUTS = " + JSON.stringify(outputs) + ";");
    }
    // static pack
    const target = pathname === "/" ? "studio.html" : pathname.replace(/^\/+/, "");
    const file = path.join(pack, target);
    if (!file.startsWith(pack) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      res.writeHead(404, { "Content-Type": "text/plain" });
      return res.end("not found");
    }
    res.writeHead(200, { "Content-Type": MIME[path.extname(file)] || "application/octet-stream" });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => resolve({ server, port: server.address().port }));
  });
}

function readBody(req, done) {
  let raw = "";
  req.on("data", (chunk) => (raw += chunk));
  req.on("end", () => done(JSON.parse(raw || "{}")));
}

(async () => {
  const browser = await chromium.launch({
    executablePath:
      process.env.CHROME_BIN ||
      (process.platform === "darwin"
        ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        : undefined),
    headless: true,
  });

  const homepage = freshState();
  const homepageServer = await startServer({ config: null, state: homepage });
  const signedOutState = freshState();
  const signedOutServer = await startServer({
    config: { issuer: "https://auth.example/realms/conveyor", client_id: "conveyor-web", dev_auth: false },
    state: signedOutState,
  });
  const snapshotState = freshState();
  const snapshotServer = await startServer({
    config: null,
    state: snapshotState,
    snapshot: SNAPSHOT,
    standards: STANDARDS,
  });
  // #386: three projects that isolate the closure-debt rule. C1 is closed and owes one
  // at-closure category; C2 is closed and complete; A1 is active and owes nothing yet.
  const closureState = freshState();
  const closureServer = await startServer({
    config: null,
    state: closureState,
    snapshot: {
      schema: "design-snapshot/v1",
      generated_at: "2026-11-19T00:00:00Z",
      git_sha: "0".repeat(40),
      tenant: "nonagentic",
      programme: {
        projects: [
          {
            id: "p-closed-owed",
            key: "C1",
            name: "Closed and owes a closure record",
            status: "closed",
            programme_id: "prog-1",
            project_type_code: "delivery",
            meta: {
              tranche: 1,
              expected_outputs: ["project-brief"],
              closure_outputs: ["evidence", "closure-review"],
            },
            version: 1,
          },
          {
            id: "p-closed-done",
            key: "C2",
            name: "Closed and complete",
            status: "closed",
            programme_id: "prog-1",
            project_type_code: "delivery",
            meta: {
              tranche: 2,
              expected_outputs: ["project-brief"],
              closure_outputs: ["evidence"],
            },
            version: 1,
          },
          {
            id: "p-active-owed",
            key: "A1",
            name: "Active, closure not owed yet",
            status: "active",
            programme_id: "prog-1",
            project_type_code: "delivery",
            meta: {
              tranche: 3,
              expected_outputs: ["project-brief"],
              closure_outputs: ["evidence"],
            },
            version: 1,
          },
        ],
      },
      organisation: {},
      config: {},
      work: {},
      msp: {},
      artefacts: [],
      measure: {},
    },
    outputs: {
      schema: "design-outputs/v1",
      outputs: [
        { project_id: "p-closed-owed", category: "project-brief", title: "C1 brief", publishable: true },
        { project_id: "p-closed-owed", category: "evidence", title: "C1 evidence", publishable: true },
        { project_id: "p-closed-done", category: "project-brief", title: "C2 brief", publishable: true },
        { project_id: "p-closed-done", category: "evidence", title: "C2 evidence", publishable: true },
        { project_id: "p-active-owed", category: "project-brief", title: "A1 brief", publishable: true },
      ],
    },
  });
  const consoleState = freshState();
  const signedIn = await startServer({
    config: {
      issuer: "",
      client_id: "conveyor-web",
      dev_auth: false,
      console_dev_token: "stub-token",
      console_tenant: "nonagentic",
    },
    state: consoleState,
  });

  try {
    /* 1 — the homepage/file:// shape: no config, no API, no controls, DESIGN default. */
    {
      const page = await browser.newPage();
      await page.goto(`http://127.0.0.1:${homepageServer.port}/studio.html#plan`);
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      assert.equal(await page.locator("#console-chip").count(), 0, "no chip without a config");
      assert.match(
        await page.locator("#main").innerText(),
        /Boards/,
        "the public plan opens on Boards",
      );
      assert.equal(
        await page.locator('[data-v2="tab"][data-key="plan"][data-value="WBS"]').count(),
        0,
        "the WBS study tab is not public",
      );
      await page.waitForSelector("text=No published snapshot is loaded");
      assert.equal(await page.locator('[data-v2="console-signin"]').count(), 0, "no sign-in without a config");
      assert.equal(
        await page.locator('[data-v2="console-intake"], [data-v2="console-card"]').count(),
        0,
        "no write control without a config",
      );
      assert.equal(homepage.requests.length, 0, "no API request without a config");
      await page.goto(`http://127.0.0.1:${homepageServer.port}/studio.html#projects`);
      await page.waitForSelector(".page");
      assert.equal(
        await page.locator('[data-v2="console-signin"]').count(),
        0,
        "no sign-in on the homepage projects page",
      );
      assert.equal(
        await page
          .locator(
            '[data-v2="project-new"], [data-v2="project-edit"], [data-v2="project-close"], [data-v2="project-execute"], [data-v2="output-new"], [data-v2="publish-start"]',
          )
          .count(),
        0,
        "no workspace write control without a config",
      );
      assert.equal(homepage.requests.length, 0, "no API request from the homepage projects page");
      await page.screenshot({ path: shot("console-homepage.png"), fullPage: true });
      await page.close();
    }

    /* 1b — the app origin, signed out: sign-in is offered, still zero API requests. */
    {
      const page = await browser.newPage();
      await page.goto(`http://127.0.0.1:${signedOutServer.port}/studio.html#plan`);
      await page.waitForSelector('#console-chip[data-state="out"]');
      assert.match(await page.locator("#main").innerText(), /No published snapshot is loaded/);
      await page.waitForSelector('[data-v2="console-signin"]');
      assert.equal(
        await page.locator('[data-v2="console-intake"], [data-v2="console-card"]').count(),
        0,
        "no write control before signing in",
      );
      assert.equal(signedOutState.requests.length, 0, "no API request before signing in");
      await page.goto(`http://127.0.0.1:${signedOutServer.port}/studio.html#projects`);
      await page.waitForSelector('[data-v2="console-signin"]');
      assert.equal(
        await page
          .locator(
            '[data-v2="project-new"], [data-v2="project-edit"], [data-v2="project-close"], [data-v2="project-execute"], [data-v2="output-new"], [data-v2="publish-start"]',
          )
          .count(),
        0,
        "no workspace write control before signing in",
      );
      assert.equal(signedOutState.requests.length, 0, "no API request on the signed-out projects page");
      await page.goto(`http://127.0.0.1:${signedOutServer.port}/studio.html#execution`);
      await page.waitForSelector(".page");
      assert.equal(signedOutState.requests.length, 0, "no API request on the signed-out execution page");
      assert.equal(
        await page.locator('[data-v2="run-cancel"], [data-v2="approval-approve"]').count(),
        0,
        "no execution control before signing in",
      );
      await page.close();
    }

    /* 1c — the public pack with a snapshot: real rows, zero API, contribute units. */
    {
      const page = await browser.newPage();
      const errors = [];
      page.on("pageerror", (error) => errors.push(String(error)));
      await page.goto(`http://127.0.0.1:${snapshotServer.port}/studio.html#portfolio`);
      await page.waitForTimeout(600);
      assert.deepEqual(errors, [], "no page error on the public live portfolio");
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      const portfolioText = await page.locator("#main").innerText();
      assert.match(portfolioText, /S02/, "the live portfolio renders the snapshot's project");
      assert.match(portfolioText, /Skeleton/, "the real project name renders");
      assert.match(portfolioText, /Programme queue/, "the public queue renders");
      assert.doesNotMatch(
        portfolioText,
        /Identity & access|Citizen services/,
        "no fixture project on the live portfolio",
      );
      await page.goto(`http://127.0.0.1:${snapshotServer.port}/studio.html#projects`);
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      const publicProjectText = await page.locator("#main").innerText();
      assert.match(
        publicProjectText,
        /Expected outputs/,
        "the public project shows the standard's checklist",
      );
      assert.doesNotMatch(
        publicProjectText,
        /present, private/,
        "no private draft is public",
      );
      // #528: the four delivery levels, on the same published snapshot.
      await page.goto(`http://127.0.0.1:${snapshotServer.port}/studio.html#portfolio`);
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      const timeline = await page.locator("#main").innerText();
      assert.match(timeline, /Delivery timeline · portfolio/, "L1 renders the portfolio timeline");
      assert.match(timeline, /Delivery programme/, "a programme row renders at L1");
      assert.match(timeline, /Standalone initiative/, "a standalone initiative renders at L1");
      assert.match(timeline, /1\.5M tok/, "the programme's token total renders");
      assert.match(timeline, /\$0\.42\*/, "the derived cost renders, marked unpriced");
      assert.match(timeline, /unpriced/, "an unpriced call is marked, never priced as free");
      // MSP at portfolio level: the programme row carries its tranche and benefit state.
      assert.match(timeline, /T0 closed · T1–2 planned/, "the programme row carries its tranches");
      assert.match(timeline, /1\/2 benefits measured/, "the programme row carries its benefits");
      assert.match(timeline, /1 completed/, "completed work is counted at L1");
      assert.ok(
        (await page.locator("#main .gantt .gantt-row").count()) >= 3,
        "L1 has a row per programme and standalone initiative",
      );
      assert.ok(
        (await page.locator("#main .gantt-actual").count()) >= 2,
        "the actual bars are drawn",
      );
      // An in-flight programme's bar reaches the snapshot clock; the 0.8 % sliver is the regression.
      const barWidth = await page
        .locator('#main .gantt-row[data-state="in_progress"] .gantt-actual')
        .first()
        .evaluate((el) => parseFloat(el.style.width));
      assert.ok(barWidth > 20, `an in-flight programme bar spans its window (${barWidth}%)`);
      // #528: L1's standards scorecard — the audit's own record per standard, with an open-package
      // and findings count on each tile.
      assert.ok(
        (await page.locator("#main .delivery-scorecard").count()) >= 1,
        "L1 carries the standards scorecard",
      );
      const scorecardText = await page.locator("#main .delivery-scorecard").innerText();
      assert.match(scorecardText, /MSP/, "the scorecard scores MSP");
      assert.match(scorecardText, /TOGAF/, "the scorecard scores TOGAF");
      assert.match(scorecardText, /2 open · 1 findings/, "an open-package count renders");
      // #528: the generated standards record at L1 — per-standard evidenced/partial/absent counts.
      const standardsSummary = await page.locator("#main .std-rail").first().innerText();
      assert.match(standardsSummary, /MSP 0\/1 evidenced/, "L1 counts the MSP artefacts");
      assert.match(standardsSummary, /TOGAF 0\/1 evidenced/, "L1 counts the TOGAF artefacts");
      assert.match(standardsSummary, /1 partial/, "L1 names the partial artefact");
      assert.match(standardsSummary, /1 absent/, "L1 names the absent artefact");
      // L2: the programme drill — only its projects, grouped by MSP tranche.
      await page.goto(
        `http://127.0.0.1:${snapshotServer.port}/studio.html#portfolio/11111111-1111-7111-8111-111111111111`,
      );
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      const programmeTimeline = await page.locator("#main").innerText();
      assert.match(
        programmeTimeline,
        /Delivery timeline · programme/,
        "L2 renders the programme timeline",
      );
      assert.match(programmeTimeline, /Delivery project/, "the programme's project renders");
      const programmeRows = await page
        .locator("#main .gantt-row .gantt-name")
        .allInnerTexts();
      assert.ok(
        programmeRows.includes("Delivery project"),
        "L2's timeline carries the programme's project",
      );
      assert.ok(
        !programmeRows.includes("Standalone initiative"),
        "L2's timeline shows only this programme's projects",
      );
      assert.ok(
        (await page.locator("#main .gantt-group").count()) >= 3,
        "L2 groups the projects by their MSP tranche",
      );
      assert.match(programmeTimeline, /tranche 1/i, "the tranche header renders");
      assert.match(programmeTimeline, /MSP tranches/i, "the tranche rail renders");
      assert.match(programmeTimeline, /BEN-P0/, "the measured benefit renders");
      assert.match(programmeTimeline, /BEN-P1/, "the unmeasured benefit renders");
      assert.match(programmeTimeline, /unmeasured/, "an unmeasured baseline says so");
      assert.match(programmeTimeline, /no dated activity/, "a project with no event has no bar");
      assert.match(programmeTimeline, /—\*/, "an unpriced-only spend renders —, never $0.00");
      assert.match(programmeTimeline, /Closed project/, "a completed project stays visible at L2");
      assert.match(programmeTimeline, /MSP benefits/i, "the benefit rail renders");
      // #528: L2's derived rails — the declaration register, the reconciliation and TOGAF phases,
      // and the realisation rows that replace the profile-only benefit state.
      assert.ok(
        (await page.locator("#main .delivery-rail.phase-rail").count()) >= 1,
        "L2 carries the TOGAF phase rail",
      );
      assert.match(programmeTimeline, /dependencies/i, "the dependency rail renders");
      assert.match(programmeTimeline, /1 typed · 1 untyped/, "typed and untyped edges are counted");
      assert.match(programmeTimeline, /unknown targets/i, "an unknown dependency target is named");
      assert.match(programmeTimeline, /S02→GONE/, "the unknown target edge is shown");
      assert.match(programmeTimeline, /tranche reconciliation/i, "the reconciliation rail renders");
      assert.match(programmeTimeline, /not authorised/, "a tranche with no authorisation says so");
      assert.match(programmeTimeline, /1 flag/, "a reconciliation flag is counted");
      assert.match(programmeTimeline, /evidenced/, "a TOGAF phase state renders");
      assert.match(programmeTimeline, /not-evidenced/, "an unevidenced phase says so");
      assert.match(programmeTimeline, /next b/, "the next required phase is named");
      assert.match(programmeTimeline, /realised/, "a realised benefit renders");
      assert.match(programmeTimeline, /unbaselined/, "an unbaselined benefit says so");
      // #528: L2's generated standards rail — the programme artefact, its status and its package.
      const programmeStandards = await page.locator("#main .std-rail").first().innerText();
      assert.match(programmeStandards, /vision · Delivery programme/, "L2 carries the programme artefact");
      assert.match(programmeStandards, /partial/i, "the artefact's status renders");
      assert.match(programmeStandards, /std-msp-vision-delivery:programme/, "the package that must close renders");
      assert.ok(
        (await page.locator('#main a[href="#portfolio"]').count()) >= 1,
        "L2 links back to the portfolio",
      );
      // L3: the project kanban — the work package, the loose card and the archived card, with the
      // project's TOGAF/PMP record above it.
      await page.goto(
        `http://127.0.0.1:${snapshotServer.port}/studio.html#projects/22222222-2222-7222-8222-222222222222`,
      );
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      const boardText = await page.locator("#main").innerText();
      assert.match(boardText, /Work package Alpha/, "the work-package tile renders");
      assert.match(boardText, /WORK PACKAGE · 3/, "the tile carries its child count");
      assert.match(boardText, /Loose card/, "a card with no children renders as a card");
      assert.match(boardText, /Archived card/, "an archived card stays visible");
      assert.match(boardText, /delivery_contract/, "the TOGAF contract is on the project record");
      assert.match(boardText, /compliant/, "the compliance level is on the project record");
      assert.ok(
        (await page.locator("#main .kanban .kanban-col").count()) >= 2,
        "the board's lanes are columns",
      );
      assert.ok(
        (await page.locator("#main .kanban-col.kanban-archived .kanban-card").count()) >= 1,
        "archived work has its own column",
      );
      assert.ok(
        (await page.locator('#main .kanban-card a.kanban-open[href*="/wp/"]').count()) >= 3,
        "every card opens its own work-package view",
      );
      // #528: L3's Plan and Rework lanes — the WBS work package, the applied rework card, and the
      // register package that has no card yet.
      assert.ok(
        (await page.locator("#main .kanban-col.kanban-rework").count()) >= 1,
        "the project board carries its Rework lane",
      );
      const reworkLane = await page.locator("#main .kanban-col.kanban-rework").innerText();
      assert.match(reworkLane, /P0 · contract_missing/, "the applied package renders as a rework card");
      assert.match(
        reworkLane,
        /not yet materialised/,
        "a register package with no card is not materialised",
      );
      const planLane = await page.locator("#main .kanban-col.kanban-plan").innerText();
      assert.match(planLane, /Deliver the pack/, "the Plan lane carries the WBS work package");
      // #528: L3's generated standards rail — the project's phase artefact and its package.
      const projectStandards = await page.locator("#main .std-rail").first().innerText();
      assert.match(projectStandards, /phase-a · DLV — A/, "L3 carries the project's phase artefact");
      assert.match(projectStandards, /std-togaf-phase-a-dlv:DLV/, "the absent phase names its package");
      // L4: the work package's execution and its own kanban, including the done child.
      await page.goto(
        `http://127.0.0.1:${snapshotServer.port}/studio.html#projects/22222222-2222-7222-8222-222222222222/wp/66666666-6666-7666-8666-666666666666`,
      );
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      const packageText = await page.locator("#main").innerText();
      assert.match(packageText, /Child one/, "the work package's first child renders");
      assert.match(packageText, /Child two/, "the work package's second child renders");
      assert.match(packageText, /2\/3 cards done/, "the work package shows its progress");
      assert.match(packageText, /execution/i, "the work package shows its execution");
      assert.match(packageText, /delivery\.review/, "the card's gate is shown");
      assert.match(packageText, /2 decisions/, "the gate's required decisions are shown");
      assert.match(packageText, /READY: the card is scoped/, "the Definition of Done is shown");
      assert.match(packageText, /1 run/, "the card's run is shown");
      assert.match(packageText, /Child three/, "an archived child renders");
      // #528: L4's generated standards rail — the card's evidence artefact and its status.
      const cardStandards = await page.locator("#main .std-rail").first().innerText();
      assert.match(cardStandards, /evidence · Evidence card/, "L4 carries the card's evidence artefact");
      assert.match(cardStandards, /evidenced/i, "the card artefact's status renders");
      assert.equal(
        (packageText.match(/Child three/g) || []).length,
        1,
        "an archived child renders once (Archived column, not also its old lane)",
      );
      assert.equal(
        await page.locator('#main .kanban-col[data-lane="active"] .kanban-card').count(),
        1,
        "the archived child is not repeated in its previous lane",
      );
      assert.equal(
        await page.locator('#main .kanban-col[data-lane="archived"] .kanban-card').count(),
        1,
        "the archived child has its own column",
      );
      assert.ok(
        (await page.locator('#main a[href="#projects/22222222-2222-7222-8222-222222222222"]').count()) >= 1,
        "the work package links back to the project board",
      );
      // L4 for a childless card: the execution is the answer, not an empty board.
      await page.goto(
        `http://127.0.0.1:${snapshotServer.port}/studio.html#projects/22222222-2222-7222-8222-222222222222/wp/88888888-8888-7888-8888-888888888888`,
      );
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      const childlessText = await page.locator("#main").innerText();
      assert.match(childlessText, /No child cards/, "a childless work package says so");
      assert.match(childlessText, /No run is recorded/, "its empty execution is honest");
      assert.match(childlessText, /definition of done/i, "it still carries its Definition of Done");
      // #528: the loose card is the applied rework card; L4 leads with its register package.
      await page.goto(
        `http://127.0.0.1:${snapshotServer.port}/studio.html#projects/22222222-2222-7222-8222-222222222222/wp/88888888-8888-7888-8888-888888888888`,
      );
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      assert.ok(
        (await page.locator("#main .exec.rework-package").count()) >= 1,
        "the work package shows its rework register package",
      );
      const reworkPackageText = await page.locator("#main .exec.rework-package").innerText();
      assert.match(reworkPackageText, /contract_missing/, "the package's rule is shown");
      assert.match(reworkPackageText, /MSP · MSP §8\.3/, "the standard and SPEC clause are shown");
      assert.match(
        reworkPackageText,
        /the tranche-4 delivery contract is published/,
        "the package's Definition of Done item is shown",
      );
      assert.match(
        reworkPackageText,
        /python3 scripts\/audit-programme-conformance\.py --snapshot docs\/design\/data --report/,
        "the verify command is shown",
      );
      // A phone gets the same view, scrolled inside its own box: the page itself never overflows.
      await page.setViewportSize({ width: 390, height: 844 });
      await page.waitForTimeout(150);
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - window.innerWidth,
      );
      assert.ok(overflow <= 1, `the page has no horizontal overflow at 390px (${overflow})`);
      await page.setViewportSize({ width: 1440, height: 1000 });
      // A project with no board says so; it never renders empty columns. QUIET carries no board;
      // LONE now carries one because the §5.1 boundary admits it to L1 only with a board or card.
      await page.goto(
        `http://127.0.0.1:${snapshotServer.port}/studio.html#projects/99999999-9999-7999-8999-999999999999`,
      );
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      const noBoardText = await page.locator("#main").innerText();
      assert.match(noBoardText, /no board/i, "a project with no board says so");
      assert.equal(
        await page.locator("#main .kanban-col").count(),
        0,
        "a project with no board renders no columns",
      );
      // #526: the five reopened delivery-view findings, asserted on the module's own renderers.
      // Each fixture is the smallest snapshot shape that exercises the reported branch.
      const views = await page.evaluate(() => {
        const D = window.CONVEYOR_DELIVERY;
        const clock = Date.parse("2026-11-20T00:00:00Z");
        // v1-view-1: no work.project_activity; one project has a card and a run, one has neither.
        const observedWork = {
          boards: [{ id: "b-ob", project_id: "p-ob" }],
          cards: [
            {
              id: "c-ob",
              board_id: "b-ob",
              created_at: "2026-11-01T00:00:00Z",
              updated_at: "2026-11-03T00:00:00Z",
            },
          ],
          runs: [
            {
              id: "r-ob",
              project_id: "p-ob",
              created_at: "2026-11-05T00:00:00Z",
              finished_at: "2026-11-06T00:00:00Z",
            },
          ],
        };
        const observedProgramme = {
          generated_at: "2026-11-20T00:00:00Z",
          projects: [
            { id: "p-ob", key: "OB", name: "Observed project", status: "active" },
            { id: "p-none", key: "NONE", name: "No card and no run", status: "active" },
          ],
        };
        const observed = D.activityIndex(observedProgramme, observedWork, {}, clock);
        // v1-view-2: totals only (no by_project/by_card), in priced, unpriced and zero shapes.
        const spend = (totals) => {
          const acc = D.tenantSpend({ inference: { totals } });
          return acc ? D.spendText(acc.tokens, acc.usd, acc.priced, acc.unpriced) : null;
        };
        const totalsOnly = {
          inference: {
            totals: {
              total_tokens: 4200000,
              cost_usd: "1.250000",
              priced_calls: 4,
              unpriced_calls: 2,
            },
          },
        };
        const spendWork = {
          boards: [{ id: "b-sp", project_id: "p-sp" }],
          lists: [{ id: "l-sp", board_id: "b-sp", kind: "active" }],
          cards: [{ id: "c-sp", board_id: "b-sp", list_id: "l-sp", name: "Spend card" }],
        };
        const spendBoard = D.boardModel(spendWork, { id: "p-sp", key: "SP" }, totalsOnly, {}, {});
        // v1-view-3: standalone membership needs active/paused AND a board or a card.
        const standaloneProgramme = {
          generated_at: "2026-11-20T00:00:00Z",
          programme: [],
          projects: [
            { id: "p-a", key: "PA", name: "Active with board", status: "active" },
            { id: "p-p", key: "PP", name: "Paused with card", status: "paused" },
            { id: "p-c", key: "PC", name: "Closed with board", status: "closed" },
            { id: "p-d", key: "PD", name: "Draft with board", status: "draft" },
            { id: "p-e", key: "PE", name: "Archived with board", status: "archived" },
            { id: "p-b", key: "PB", name: "Active but bare", status: "active" },
          ],
        };
        const standaloneWork = {
          boards: [
            { id: "b-a", project_id: "p-a" },
            { id: "b-p", project_id: "p-p" },
            { id: "b-c", project_id: "p-c" },
            { id: "b-d", project_id: "p-d" },
            { id: "b-e", project_id: "p-e" },
          ],
          lists: [
            { id: "l-a", board_id: "b-a", kind: "active" },
            { id: "l-p", board_id: "b-p", kind: "active" },
            { id: "l-c", board_id: "b-c", kind: "active" },
            { id: "l-d", board_id: "b-d", kind: "active" },
            { id: "l-e", board_id: "b-e", kind: "active" },
          ],
          cards: [{ id: "c-p", board_id: "b-p", list_id: "l-p", name: "Paused card" }],
        };
        const standalone = D.portfolioRows(
          standaloneProgramme,
          D.activityIndex(standaloneProgramme, standaloneWork, {}, clock),
          clock,
          {},
        );
        // v1-view-4: twelve benefit profiles; the old rail stopped at eight.
        const benefits = Array.from({ length: 12 }, (_, i) => ({
          reference: "BEN-" + String(i + 1).padStart(2, "0"),
          description: "Benefit " + (i + 1),
          target: "target " + (i + 1),
        }));
        // v1-view-5: 60 cards in one lane, 30 in another, 30 archived.
        const cards = [];
        for (let i = 0; i < 60; i += 1) {
          cards.push({ id: "a-" + i, board_id: "b-kb", list_id: "l-kb-1", name: "Lane one " + i });
        }
        for (let i = 0; i < 30; i += 1) {
          cards.push({ id: "b-" + i, board_id: "b-kb", list_id: "l-kb-2", name: "Lane two " + i });
        }
        for (let i = 0; i < 30; i += 1) {
          cards.push({
            id: "z-" + i,
            board_id: "b-kb",
            list_id: "l-kb-1",
            name: "Archived " + i,
            archived_at: "2026-11-10T00:00:00Z",
          });
        }
        const kanbanHtml = D.kanban(
          D.boardModel(
            {
              boards: [{ id: "b-kb", project_id: "p-kb", position: 0 }],
              lists: [
                { id: "l-kb-1", board_id: "b-kb", name: "One", kind: "active", position: 1 },
                { id: "l-kb-2", board_id: "b-kb", name: "Two", kind: "active", position: 2 },
                { id: "l-kb-done", board_id: "b-kb", name: "Done", kind: "done", position: 3 },
              ],
              cards,
            },
            { id: "p-kb", key: "KB" },
            {},
            {},
            {},
          ),
        );
        const holder = document.createElement("div");
        holder.innerHTML = kanbanHtml;
        return {
          view1: {
            start: observed.get("p-ob").start,
            end: observed.get("p-ob").end,
            quietStart: observed.get("p-none").start,
            html: D.gantt([...observed.values()]),
          },
          view2: {
            note:
              (D.kanban(spendBoard).match(/<p class="muted kanban-note">([^<]*)<\/p>/) || [])[1] || "",
            attributed: D.tenantSpend({
              inference: {
                by_project: [{ project_id: "p", total_tokens: 10 }],
                totals: totalsOnly.inference.totals,
              },
            }),
            priced: spend({ total_tokens: 1000000, cost_usd: "0.500000", priced_calls: 2, unpriced_calls: 0 }),
            unpriced: spend({ total_tokens: 5000, cost_usd: "0.000000", priced_calls: 0, unpriced_calls: 3 }),
            zero: spend({ total_tokens: 0, cost_usd: "0.000000", priced_calls: 0, unpriced_calls: 0 }),
            absent: D.tenantSpend({ inference: {} }),
          },
          view3: standalone.standalone.map((row) => row.key),
          view4: {
            html: D.benefitRail({ benefits }),
            count: (D.benefitRail({ benefits }).match(/<span class="rail-chip/g) || []).length,
          },
          view5: [...holder.querySelectorAll(".kanban-col")].map((col) => ({
            lane: col.getAttribute("data-lane"),
            archived: col.classList.contains("kanban-archived"),
            tiles: col.querySelectorAll(".kanban-card").length,
            more: [...col.querySelectorAll(".kanban-more")].map((el) => el.textContent.trim()),
          })),
        };
      });
      // v1-view-1: the window is derived from the card and the run when project_activity is absent.
      assert.equal(
        views.view1.start,
        Date.parse("2026-11-01T00:00:00Z"),
        "v1-view-1: the earliest card event opens the derived window",
      );
      assert.equal(
        views.view1.end,
        Date.parse("2026-11-20T00:00:00Z"),
        "v1-view-1: an open project's derived window reaches the snapshot clock",
      );
      assert.equal(
        views.view1.quietStart,
        null,
        "v1-view-1: a project with no card and no run has no window",
      );
      assert.equal(
        (views.view1.html.match(/gantt-actual/g) || []).length,
        1,
        "v1-view-1: the observed project draws one bar",
      );
      assert.equal(
        (views.view1.html.match(/gantt-nodate/g) || []).length,
        1,
        "v1-view-1: a project with no card and no run gets no bar",
      );
      // v1-view-2: the tenant total is named unattributed, in priced, unpriced and zero shapes.
      assert.match(
        views.view2.note,
        /^4\.2M tok · \$1\.25\* unattributed to a project\.$/,
        "v1-view-2: tenant-only spend is reported as unattributed",
      );
      assert.equal(
        views.view2.attributed,
        null,
        "v1-view-2: attributed spend is not duplicated as unattributed",
      );
      assert.match(views.view2.priced, /^1\.0M tok · \$0\.50$/, "v1-view-2: a priced total renders dollars");
      assert.match(
        views.view2.unpriced,
        /^5\.0k tok · —\*$/,
        "v1-view-2: an unpriced total renders —*, never $0.00",
      );
      assert.match(views.view2.zero, /^0 tok · \$0\.00$/, "v1-view-2: a zero total is a real zero");
      assert.equal(views.view2.absent, null, "v1-view-2: a missing totals row is not a spend claim");
      // v1-view-3: only an active/paused standalone row with a board or a card is public.
      assert.deepEqual(
        views.view3,
        ["PA", "PP"],
        "v1-view-3: closed, draft, archived and bare standalone rows are excluded",
      );
      // v1-view-4: every benefit profile renders; the old rail stopped at eight.
      assert.equal(views.view4.count, 12, "v1-view-4: all twelve benefit profiles render");
      assert.match(views.view4.html, /BEN-09/, "v1-view-4: the ninth profile is not dropped");
      assert.match(views.view4.html, /BEN-12/, "v1-view-4: the twelfth profile is not dropped");
      // v1-view-5: every lane and the archived column cap at 25 with a visible remainder.
      assert.ok(
        views.view5
          .filter((col) => ["active", "done", "archived"].includes(col.lane))
          .every((col) => col.tiles <= 25),
        "v1-view-5: no board lane renders more than 25 tiles",
      );
      assert.deepEqual(
        views.view5
          .filter((col) => col.lane === "active")
          .map((col) => [col.tiles, col.more.join("")]),
        [
          [25, "35 more"],
          [25, "5 more"],
        ],
        "v1-view-5: both crowded lanes cap at 25 and carry their remainder",
      );
      const archivedColumn = views.view5.find((col) => col.archived);
      assert.deepEqual(
        [archivedColumn.tiles, archivedColumn.more.join("")],
        [25, "5 more"],
        "v1-view-5: the archived column caps at 25 and carries its remainder",
      );
      await page.goto(`http://127.0.0.1:${snapshotServer.port}/studio.html#contribute`);
      await page.waitForSelector(".page");
      await page.waitForTimeout(200);
      const contributeText = await page.locator("#main").innerText();
      assert.match(contributeText, /Marked ready/, "contribute renders the ready summary");
      assert.match(contributeText, /verify on GitHub/, "the ready count is labelled a snapshot");
      assert.match(contributeText, /#2/, "a ready unit links its issue");
      assert.match(contributeText, /Unassigned/, "an unlinked ready unit is grouped honestly");
      assert.match(contributeText, /How a unit is claimed/, "the claim protocol renders");
      // #434: the Operations destination reads the incident record from the snapshot. It is a
      // design destination (not announced to a signed-out visitor), so the reviewer mode
      // `?design=1` is how its live branch renders without a sign-in.
      await page.goto(
        `http://127.0.0.1:${snapshotServer.port}/studio.html?design=1#operations`,
      );
      await page.waitForSelector(".page");
      await page.waitForTimeout(200);
      const operationsText = await page.locator("#main").innerText();
      assert.match(
        operationsText,
        /Incident: checkout-api \(sev2\)/,
        "the live operations screen renders the incident card",
      );
      assert.match(operationsText, /Crash-loop runbook/, "the runbook selection renders its procedure title");
      assert.match(operationsText, /Postmortem: crash loop/, "the linked postmortem renders");
      assert.doesNotMatch(
        operationsText,
        /operator text that must not be public/,
        "no operator narrative is public",
      );
      assert.equal(snapshotState.requests.length, 0, "the public pack makes no API request");
      // #305: the published msp snapshot drives the programme definition and plan spine read-only.
      await page.click('[data-action="navigate"][data-page="programme"]');
      await page.waitForSelector('[data-v2="tab"][data-key="programme"][data-value="Definition"]');
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("conveyor buildout"),
      );
      const definitionText = await page.locator("#main").innerText();
      assert.match(definitionText, /Vision and mandate/, "the definition set renders");
      assert.doesNotMatch(
        definitionText,
        /not publishable/,
        "#333: a snapshot row without the member does not invent an unpublished state",
      );
      await page.click('[data-v2="tab"][data-key="programme"][data-value="Benefits map & profiles"]');
      await page.waitForFunction(() => document.querySelector("#main").innerText.includes("BEN-P0"));
      assert.match(await page.locator("#main").innerText(), /A published benefit/, "the snapshot benefit renders");
      await page.click('[data-v2="tab"][data-key="programme"][data-value="Tranches & reviews"]');
      await page.waitForFunction(() => document.querySelector("#main").innerText.includes("BL-P0"));
      await page.click('[data-v2="tab"][data-key="programme"][data-value="Benefits realization"]');
      await page.waitForFunction(() => document.querySelector("#main").innerText.includes("BEN-P0"));
      await page.click('[data-v2="tab"][data-key="programme"][data-value="Stakeholders"]');
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("Published sponsor"),
      );
      assert.equal(
        await page
          .locator(
            '[data-v2="msp-save"], [data-v2="msp-delete"], [data-v2="msp-benefit-new"], [data-v2="msp-tranche-new"], [data-v2="msp-stakeholder-new"], [data-v2="msp-wbs-new"], [data-v2="msp-register-new"], [data-v2="msp-cadence-new"], [data-v2="msp-baseline-take"]',
          )
          .count(),
        0,
        "the snapshot programme is read-only",
      );
      await page.goto(`http://127.0.0.1:${snapshotServer.port}/studio.html#plan`);
      await page.waitForSelector('[data-v2="tab"][data-key="plan"][data-value="WBS"]');
      await page.click('[data-v2="tab"][data-key="plan"][data-value="WBS"]');
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("Published WBS node"),
      );
      await page.click('[data-v2="tab"][data-key="plan"][data-value="Baseline vs forecast"]');
      await page.waitForFunction(() => document.querySelector("#main").innerText.includes("bl-p0"));
      await page.click('[data-v2="tab"][data-key="plan"][data-value="Registers"]');
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("Published register risk"),
      );
      await page.click('[data-v2="tab"][data-key="plan"][data-value="Cadence"]');
      await page.waitForFunction(() => document.querySelector("#main").innerText.includes("PI 2026.2"));
      assert.equal(
        await page
          .locator(
            '[data-v2="msp-save"], [data-v2="msp-delete"], [data-v2="msp-wbs-new"], [data-v2="msp-register-new"], [data-v2="msp-cadence-new"], [data-v2="msp-baseline-take"]',
          )
          .count(),
        0,
        "the snapshot plan spine is read-only",
      );
      assert.equal(snapshotState.requests.length, 0, "the snapshot programme pages make no API request");
      // #385: the architecture capability reads the published snapshot read-only. A pasted
      // hash is a fresh load: the pack reads its destination at boot.
      await page.goto(`http://127.0.0.1:${snapshotServer.port}/studio.html#adm`);
      await page.reload();
      await page.waitForSelector(".page");
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("Published tailored ADM profile"),
      );
      const snapshotAdm = await page.locator("#main").innerText();
      assert.match(
        snapshotAdm,
        /Published Architecture Board/,
        "the published board renders on the capability tab",
      );
      assert.match(
        snapshotAdm,
        /Architecture process/,
        "the newest published maturity baseline renders",
      );
      assert.match(
        await page.locator("#main .v2-ribbon").first().innerText(),
        /LIVE/,
        "the capability ribbon names the snapshot source",
      );
      assert.equal(
        await page
          .locator(
            '[data-v2="adm-advance"], [data-v2="adm-approve"], [data-v2="adm-h-change"], [data-v2="adm-h-kata"], [data-v2="adm-review"], [data-v2="adm-review-second"], [data-v2="adm-waiver"], [data-v2="adm-waiver-ok"]',
          )
          .count(),
        0,
        "the published capability carries no write control",
      );
      await page.click('[data-v2="tab"][data-value="Artefact repository"]');
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("Published principles catalog"),
      );
      assert.match(
        await page.locator("#main").innerText(),
        /One record per fact/,
        "the published artefact's entries render",
      );
      assert.equal(
        snapshotState.requests.length,
        0,
        "the snapshot capability makes no API request",
      );
      // T16: the Procedures destination is public and read-only on the snapshot.
      await page.goto(`http://127.0.0.1:${snapshotServer.port}/studio.html#procedures`);
      await page.reload();
      await page.waitForSelector(".page");
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("Synthetic receiving SOP"),
      );
      const snapshotProcedures = await page.locator("#main").innerText();
      assert.match(snapshotProcedures, /published/i, "the published procedure rows render");
      assert.doesNotMatch(snapshotProcedures, /pko/i, "the published page claims no PKO store");
      assert.equal(
        await page
          .locator(
            '[data-v2="proc-ingest-sop"], [data-v2="proc-ingest-bpmn"], [data-v2="proc-gate-process"], [data-v2="proc-publish-process"]',
          )
          .count(),
        0,
        "the snapshot procedures page carries no write control",
      );
      assert.equal(
        snapshotState.requests.length,
        0,
        "the snapshot procedures page makes no API request",
      );
      await page.close();
    }

    /* 1d — #386: a closed project's unpublished at-closure output is debt. */
    {
      const page = await browser.newPage();
      await page.goto(`http://127.0.0.1:${closureServer.port}/studio.html#portfolio`);
      await page.waitForSelector(".page");
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("Programme queue"),
      );
      const queue = page.locator("section.e-panel", { hasText: "Programme queue" });
      const queueText = await queue.innerText();
      assert.match(
        queueText,
        /0 outputs due · 1 closure output owed/,
        "the queue badge reports the closed project's closure debt",
      );
      const owedRow = await queue.locator("tr", { hasText: "C1" }).innerText();
      assert.match(owedRow, /missing: closure-review/, "the missing at-closure category is named");
      assert.doesNotMatch(
        owedRow,
        /missing: evidence/,
        "the published at-closure category is not called missing",
      );
      assert.doesNotMatch(
        await queue.locator("tr", { hasText: "C2" }).innerText(),
        /missing:/,
        "a closed project whose closure set is published has no debt",
      );
      assert.doesNotMatch(
        await queue.locator("tr", { hasText: "A1" }).innerText(),
        /missing:/,
        "an active project's missing at-closure output is not debt",
      );
      assert.equal(
        closureState.requests.length,
        0,
        "the public queue reads the snapshot, not the API",
      );
      await page.close();
    }

    /* 2 — the console: sign in, read the tenant, intake/move/comment, honest failures. */
    {
      const page = await browser.newPage();
      const errors = [];
      page.on("pageerror", (error) => errors.push(String(error)));
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#plan`);
      await page.waitForSelector('#console-chip[data-state="in"]');
      assert.match(await page.locator("#console-chip").innerText(), /Test Operator/);
      await page.waitForSelector('[data-v2-input="plan-project"]');
      const selectText = await page.locator('[data-v2-input="plan-project"]').innerText();
      assert.match(selectText, /S02/, "the project select lists the tenant's projects");

      // intake issue 3
      const intake = page.locator('[data-v2="console-intake"][data-issue="3"]');
      await intake.waitFor();
      await intake.click();
      await page.waitForSelector("text=Issue #3 intaken as a card");
      const create = consoleState.requests.find((row) => row.path === "/api/actions/cards.create/invocations");
      assert.ok(create, "cards.create was sent");
      assert.equal(create.headers["x-active-tenant"], "nonagentic", "the tenant header is sent");
      assert.match(create.headers.authorization || "", /^Bearer /, "the bearer is sent");
      await page.waitForSelector('[data-v2="console-card"][data-id="c-2"]');

      // open the card, move it, comment, claim and release
      await page.click('[data-v2="console-card"][data-id="c-1"]');
      await page.waitForSelector("#console-move");
      assert.match(await page.locator(".console-dialog-body").innerText(), /Comments/);
      await page.selectOption("#console-move", "l-active");
      await page.click('[data-v2="console-move"][data-id="c-1"]');
      await page.waitForSelector("text=Card moved");
      assert.equal(consoleState.cards.find((row) => row.id === "c-1").list_id, "l-active", "the stub moved the card");

      await page.waitForTimeout(150);
      await page.click('[data-v2="console-card"][data-id="c-1"]');
      await page.waitForSelector("#console-comment");
      await page.fill("#console-comment", "verified on the console");
      await page.click('[data-v2="console-comment"][data-id="c-1"]');
      await page.waitForSelector("text=Comment recorded");
      await page.waitForSelector("text=verified on the console");
      assert.equal(consoleState.comments["c-1"][0].body, "verified on the console", "the comment body crossed the wire");

      await page.click('[data-v2="console-claim"][data-id="c-1"]');
      await page.waitForSelector("text=Card claimed");
      assert.equal(consoleState.cards.find((row) => row.id === "c-1").assignee_id, "u-1");
      await page.waitForTimeout(150);
      await page.click('[data-v2="console-release"][data-id="c-1"]');
      await page.waitForSelector("text=Claim released");
      assert.equal(consoleState.cards.find((row) => row.id === "c-1").assignee_id, null);

      // ── the programme workspace: project edit, output, feedback, publish ─────────────
      await page.click('#dialog [data-action="close-dialog"]');
      await page.waitForFunction(() => !document.getElementById("dialog").open);
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#projects`);
      await page.waitForSelector('#console-chip[data-state="in"]');
      await page.waitForSelector('[data-v2-input="project-page"]');
      assert.match(
        await page.locator('[data-v2-input="project-page"]').innerText(),
        /S02/,
        "the project page lists the tenant's projects",
      );
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/card_types.list/invocations",
        ),
        "the console reads card_types.list, not a catalogue namespace",
      );
      assert.match(
        await page.locator("#main").innerText(),
        /draft_artifact_id/,
        "a card's expected outputs render from its type definition",
      );

      await page.click('[data-v2="project-edit"][data-id="p-s02"]');
      await page.waitForSelector("#project-name");
      await page.fill("#project-name", "Skeleton — edited in the console");
      await page.click('[data-v2="project-save"][data-id="p-s02"]');
      await page.waitForSelector("text=Project updated");
      assert.ok(
        consoleState.requests.some((row) => row.path === "/api/actions/projects.update/invocations"),
        "projects.update was sent",
      );
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/projects.programme_meta/invocations",
        ),
        "projects.programme_meta was sent",
      );
      assert.equal(
        consoleState.project.name,
        "Skeleton — edited in the console",
        "the stub applied the edit",
      );

      await page.waitForSelector('[data-v2="output-new"][data-project="p-s02"]');
      await page.click('[data-v2="output-new"][data-project="p-s02"]');
      await page.waitForSelector("#output-title");
      assert.equal(
        await page.locator("#output-unit").inputValue(),
        "S02",
        "compose prefills the project key as the unit",
      );
      assert.equal(
        await page.locator("#output-category").inputValue(),
        "project-brief",
        "compose prefills the first expected output that is not yet published",
      );
      await page.fill("#output-title", "S02 design note");
      await page.fill("#output-body", "# Design note\n\nA decision recorded from the console.");
      await page.fill("#output-unit", "7");
      await page.selectOption("#output-category", "project-brief");
      await page.click('[data-v2="output-save"][data-project="p-s02"]');
      await page.waitForSelector("text=Output saved as a draft");
      assert.ok(
        consoleState.requests.some((row) => row.path === "/api/actions/documents.compose/invocations"),
        "documents.compose was sent",
      );
      await page.waitForSelector('[data-v2="output-read"][data-id="d-1"]');
      assert.match(await page.locator("#main").innerText(), /S02 design note/, "the output is listed");
      // The workspace shows the measured programme and groups the output by unit + category.
      const workspaceText = await page.locator("#main").innerText();
      assert.match(workspaceText, /Programme truth/, "the coverage panel renders");
      assert.match(workspaceText, /T1/, "the coverage row's tranche renders");
      assert.match(workspaceText, /7 · project-brief/, "the output is grouped by unit and category");
      assert.match(workspaceText, /project-brief/, "the project's expected outputs render");
      assert.match(workspaceText, /0\/3 published/, "no expected output is published yet");
      await page.click(
        '[data-v2="output-new-template"][data-category="inception-analysis"]',
      );
      await page.waitForSelector("#output-category");
      assert.equal(
        await page.locator("#output-category").inputValue(),
        "inception-analysis",
        "the inception template pre-sets its category",
      );
      assert.match(
        await page.locator("#output-body").inputValue(),
        /Reuse sources/,
        "the inception template carries the reuse prompt",
      );
      await page.click('#dialog [data-action="close-dialog"]');
      await page.waitForFunction(() => !document.getElementById("dialog").open);

      // ── the admin publish decision ──────────────────────────────────────────────────
      await page.click('[data-v2="output-publish"][data-id="d-1"]');
      await page.waitForSelector("text=Output published");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/documents.publish/invocations",
        ),
        "documents.publish was sent",
      );
      assert.equal(consoleState.documents[0].publishable, true, "the publish decision crossed the wire");

      // ── attachments: upload through documents.create, parent-bound ──────────────────
      await page.click('[data-v2="output-attach"][data-id="d-1"]');
      await page.waitForSelector("#output-attachments");
      await page.setInputFiles('[data-v2-input="output-attachment"]', {
        name: "evidence.png",
        mimeType: "image/png",
        buffer: Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      });
      await page.waitForSelector("text=Attachment uploaded");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/documents.create/invocations",
        ),
        "documents.create was sent for the attachment",
      );
      await page.click('#dialog [data-action="close-dialog"]');
      await page.waitForFunction(() => !document.getElementById("dialog").open);

      // ── history: the revision list is read, not guessed ─────────────────────────────
      await page.click('[data-v2="output-history"][data-id="d-1"]');
      await page.waitForSelector("text=v1");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/documents.revision_list/invocations",
        ),
        "documents.revision_list was sent",
      );
      await page.click('#dialog [data-action="close-dialog"]');
      await page.waitForFunction(() => !document.getElementById("dialog").open);

      // ── unpublish, then the author's publish request ────────────────────────────────
      await page.click('[data-v2="output-publish"][data-id="d-1"]');
      await page.waitForSelector("text=Output unpublished");
      await page.waitForSelector('[data-v2="output-request-publish"][data-id="d-1"]');
      await page.click('[data-v2="output-request-publish"][data-id="d-1"]');
      await page.waitForSelector("text=Publish requested");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/documents.request_publish/invocations",
        ),
        "documents.request_publish was sent",
      );
      assert.equal(consoleState.documents[0].publishable, false, "the publish flag crossed the wire");
      await page.waitForSelector("text=0/3 published");
      assert.ok(
        !consoleState.requests.some((row) => /publish\.(start|status)/.test(row.path)),
        "the console never calls the removed publish.start/publish.status path",
      );

      // ── #245: completeness, preview, the blocked publish and the coach's triage ─────
      await page.waitForSelector('[data-v2="output-edit"][data-id="d-1"]');
      await page.click('[data-v2="output-edit"][data-id="d-1"]');
      await page.waitForSelector("#output-completeness");
      const completeness = await page.locator("#output-completeness").innerText();
      assert.match(completeness, /Outcome/, "the completeness panel lists the template's headings");
      assert.match(completeness, /○/, "an absent heading is marked missing");
      await page.fill(
        "#output-body",
        "# S02 brief\n\n## Outcome\n\nO\n\n## Boundary\n\nB\n\n## Reuse sources\n\nR\n\n## Inception units\n\nI\n\n## Dependencies\n\nD\n\n## Acceptance\n\nA\n\n## Capabilities\n\nC\n\n## Plan\n\nP\n",
      );
      assert.match(
        await page.locator("#output-completeness").innerText(),
        /✓/,
        "a present heading is marked present",
      );
      await page.click('[data-v2="output-preview"][data-id="d-1"]');
      await page.waitForSelector("iframe.console-preview");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/documents.preview/invocations",
        ),
        "documents.preview was sent",
      );
      assert.match(
        (await page.locator("iframe.console-preview").getAttribute("srcdoc")) || "",
        /preview body/,
        "the preview renders the server's html",
      );
      await page.click('#dialog [data-action="close-dialog"]');
      await page.waitForFunction(() => !document.getElementById("dialog").open);

      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#outputs`);
      await page.waitForSelector('#console-chip[data-state="in"]');
      await page.waitForSelector('[data-v2="request-publish"][data-id="d-1"]');
      consoleState.failPublishIncomplete = true;
      await page.click('[data-v2="request-publish"][data-id="d-1"]');
      await page.waitForSelector("text=Publish blocked");
      const blocked = await page.locator("#dialog").innerText();
      assert.match(blocked, /Boundary/, "the 409 names a missing heading");
      assert.equal(
        await page.locator('#dialog [data-v2="request-return"]').count(),
        1,
        "the triage offers Return",
      );
      assert.equal(
        await page.locator('#dialog [data-v2="request-kata"]').count(),
        1,
        "the triage offers a kata",
      );
      await page.click('#dialog [data-action="close-dialog"]');
      await page.waitForFunction(() => !document.getElementById("dialog").open);

      await page.click('[data-v2="request-return"][data-id="d-1"]');
      await page.waitForSelector("#return-note");
      await page.fill("#return-note", "Add the boundary and the audience.");
      await page.click('[data-v2="return-save"][data-id="d-1"]');
      await page.waitForSelector("text=Returned · missing");
      const returnedDoc = consoleState.documents.find((row) => row.id === "d-1");
      assert.equal(returnedDoc.publish_requested_at, null, "the return cleared the request");
      assert.equal(
        returnedDoc.meta.rework.note,
        "Add the boundary and the audience.",
        "the note crossed the wire",
      );
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/documents.return/invocations",
        ),
        "documents.return was sent",
      );

      await page.waitForSelector('[data-v2="output-request-publish"][data-id="d-1"]');
      await page.click('[data-v2="output-request-publish"][data-id="d-1"]');
      await page.waitForSelector("text=Publish requested");
      await page.click('[data-v2="request-kata"][data-id="d-1"]');
      await page.waitForSelector("#kata-note");
      await page.fill("#kata-note", "The template has no audience section.");
      await page.click('[data-v2="kata-save"][data-id="d-1"]');
      await page.waitForSelector("text=opened");
      assert.equal(
        consoleState.lastCompose && consoleState.lastCompose.category,
        "decision",
        "the kata is a decision",
      );
      assert.equal(consoleState.lastCompose.publishable, true, "the kata is published");
      assert.equal(consoleState.lastCompose.meta.kata, true, "the kata carries its meta");
      assert.ok(
        consoleState.requests.filter(
          (row) => row.path === "/api/actions/documents.return/invocations",
        ).length >= 2,
        "the source was returned again, linking the kata",
      );
      consoleState.documents = consoleState.documents.filter(
        (row) => !(row.meta && row.meta.kata),
      );
      await page.waitForSelector('[data-v2="output-request-publish"][data-id="d-1"]');
      await page.click('[data-v2="output-request-publish"][data-id="d-1"]');
      await page.waitForSelector("text=Publish requested");

      // ── the cross-project register: unfiltered list, publish, download, delete ───────
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#outputs`);
      await page.waitForSelector('#console-chip[data-state="in"]');
      await page.waitForSelector('[data-v2="output-publish"][data-id="d-1"]');
      assert.ok(
        consoleState.listBodies.includes("register"),
        "the register calls documents.list with no project filter",
      );
      const registerText = await page.locator("#main").innerText();
      assert.match(registerText, /S02 design note/, "the register lists the output");
      assert.match(registerText, /S02 · Skeleton/, "the register names the project");
      assert.match(registerText, /7/, "the register shows the unit");
      assert.match(registerText, /project-brief/, "the register shows the category");
      assert.match(registerText, /Test Operator/, "the register shows the author");
      assert.equal(
        await page.locator('select[data-key="outputsUnit"]').count(),
        1,
        "the register has a unit filter",
      );
      await page.selectOption('select[data-key="outputsUnit"]', "7");
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("S02 design note"),
      );
      // The admin's review queue: the pending request is visible and actionable in one place.
      const reviewQueueText = await page.locator("#main").innerText();
      assert.match(reviewQueueText, /Review requests/, "the admin sees the review queue");
      assert.equal(
        await page.locator('[data-v2="request-publish"][data-id="d-1"]').count(),
        1,
        "the pending request is actionable from the queue",
      );
      await page.click('[data-v2="request-publish"][data-id="d-1"]');
      await page.waitForSelector("text=Output published");
      await page.waitForSelector('a[href="docs/output-d-1.html"]');
      assert.equal(
        consoleState.documents[0].publishable,
        true,
        "the register publishes through documents.publish",
      );
      const [downloadEvent] = await Promise.all([
        page.waitForEvent("download"),
        page.click('[data-v2="output-download"][data-id="d-1"]'),
      ]);
      assert.ok(
        downloadEvent.suggestedFilename().startsWith("output-"),
        "the download names the output",
      );
      await page.click('[data-v2="output-delete"][data-id="d-1"]');
      await page.waitForSelector('[data-v2="output-delete-confirm"]');
      await page.click('[data-v2="output-delete-confirm"]');
      await page.waitForSelector("text=Output deleted");
      assert.equal(
        consoleState.documents.filter((row) => row.kind !== "attachment").length,
        0,
        "the delete crossed the wire",
      );
      // A soft-deleted row is returned by documents.list; the register must not render it.
      consoleState.documents.push({
        id: "d-deleted",
        project_id: "p-s02",
        kind: "document",
        title: "A deleted row must not render",
        status: "deleted",
        category: "project-brief",
        unit: "7",
        publishable: false,
        version: 1,
        meta: {},
        created_by_name: "Test Operator",
      });
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#outputs`);
      await page.waitForSelector('#console-chip[data-state="in"]');
      await page.waitForTimeout(400);
      assert.ok(
        !(await page.locator("#main").innerText()).includes("A deleted row must not render"),
        "a soft-deleted row is hidden from the register",
      );

      // ── a new project's plan reaches projects.update (create has no meta) ───────────
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#projects`);
      await page.waitForSelector('[data-v2="project-new"]');
      await page.click('[data-v2="project-new"]');
      await page.waitForSelector("#project-name");
      // The class picker is generated from the same standard the Python gate reads.
      const classCount = await page.evaluate(
        () =>
          (window.CONVEYOR_OUTPUT_STANDARD.classes || []).filter(
            (cls) => !(cls.when || {}).key && cls.name !== "archived",
          ).length,
      );
      assert.equal(
        await page.locator("#project-class option").count(),
        classCount + 1,
        "the class picker follows the output standard",
      );
      await page.fill("#project-name", "A new part of the conveyor");
      await page.fill("#project-plan", "docs/plans/slices/S00-usage-inventory.md");
      // Fail closed: a project without a class is not created and never guessed into one.
      await page.click('[data-v2="project-create"]');
      await page.waitForSelector("text=Choose a delivery class for the project");
      assert.equal(consoleState.createPatches.length, 0, "no class creates nothing");
      await page.selectOption("#project-class", "v2-proposed");
      await page.fill("#project-tranche", "4");
      await page.click('[data-v2="project-create"]');
      await page.waitForSelector("text=Project created · v2-proposed");
      assert.deepEqual(
        consoleState.planPatches,
        ["docs/plans/slices/S00-usage-inventory.md"],
        "the plan reaches projects.update",
      );
      const createPatch = consoleState.createPatches.at(-1);
      assert.deepEqual(createPatch.expected_outputs, [
        "project-brief",
        "inception-analysis",
        "architecture-design",
      ]);
      assert.deepEqual(createPatch.closure_outputs, ["evidence", "closure-review"]);
      assert.equal(createPatch.track, "v2", "the class facts are written");
      assert.equal(createPatch.measured_status, "proposed");
      assert.equal(createPatch.tranche, 4, "the tranche reaches projects.programme_meta");

      await page.click('[data-v2="feedback-intake"][data-issue="9"]');
      await page.waitForSelector("text=Issue #9 intaken as a card");
      assert.ok(
        consoleState.cards.some((card) => card.meta && card.meta.issue === 9),
        "the feedback issue became a card",
      );

      // ── a project without a board opens its checklist; its class can be changed ──────
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#projects`);
      await page.waitForSelector('[data-v2-input="project-page"]');
      await page.selectOption('[data-v2-input="project-page"]', "p-noboard");
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("No board"),
      );
      const noBoardText = await page.locator("#main").innerText();
      assert.match(noBoardText, /Expected outputs/, "the checklist renders without a board");
      assert.doesNotMatch(
        noBoardText,
        /Read refused · This project has no board/,
        "a missing board is a state, not a refusal",
      );
      await page.click('[data-v2="project-edit"][data-id="p-noboard"]');
      await page.waitForSelector("#project-class");
      await page.selectOption("#project-class", "v2-built");
      await page.fill("#project-tranche", "3");
      await page.click('[data-v2="project-save"][data-id="p-noboard"]');
      await page.waitForSelector("text=Project updated");
      assert.deepEqual(
        consoleState.noBoard.meta.expected_outputs,
        ["project-brief", "evidence"],
        "the class change recomputes the due-now set",
      );
      assert.deepEqual(consoleState.noBoard.meta.closure_outputs, ["closure-review"]);
      assert.equal(consoleState.noBoard.meta.track, "v2");
      assert.equal(consoleState.noBoard.meta.measured_status, "built");
      assert.equal(consoleState.noBoard.meta.tranche, 3);
      // Back to the board-bearing project for the execution-room section below.
      await page.selectOption('[data-v2-input="project-page"]', "p-s02");
      await page.waitForSelector('[data-v2="run-start"][data-id="c-2"]');

      // The pack-PR publish path is gone from the programme workspace: publishing is the
      // output's own `publishable` flag (the toggle asserted above).
      assert.equal(
        await page.locator('[data-v2="publish-start"]').count(),
        0,
        "the console no longer offers the pack-PR publish path",
      );

      // ── the execution room: start a run from a card, read its trace, decide a gate ──
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#projects`);
      await page.waitForSelector('[data-v2-input="project-page"]');
      await page.waitForSelector('[data-v2="run-start"][data-id="c-2"]');
      assert.equal(
        await page.locator('[data-v2="project-execute"]').count(),
        0,
        "no zombie Execute control",
      );
      await page.click('[data-v2="run-start"][data-id="c-2"]');
      await page.waitForSelector("h1:has-text('Execution room')");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/process_runs.start/invocations",
        ),
        "process_runs.start was sent",
      );
      assert.ok(consoleState.runs.length >= 2, "the stub recorded the started run");
      await page.waitForSelector('[data-v2="run-open"]');

      await page.click('[data-v2="run-open"][data-id="r-1"]');
      await page.waitForSelector("text=Run r-1 · trace");
      let mainText = await page.locator("#main").innerText();
      assert.match(mainText, /cards\.get/, "the run trace renders the step action");
      assert.ok(
        (await page
          .locator('a[href="https://github.com/nonagenticai/conveyor/pull/12"]')
          .count()) > 0,
        "the step output links the pull request",
      );
      await page.click('[data-v2="approval-approve"][data-id="a-1"]');
      await page.waitForSelector("#approval-note");
      await page.fill("#approval-note", "reviewed the diff");
      await page.click('[data-v2="approval-save"][data-id="a-1"]');
      await page.waitForSelector("text=Gate approved");
      assert.equal(consoleState.approvals[0].status, "approved", "the gate decision crossed the wire");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/approvals.decide/invocations",
        ),
        "approvals.decide was sent",
      );

      await page.click('[data-v2="run-open"][data-id="r-2"]');
      await page.waitForSelector('[data-v2="run-cancel"][data-id="r-2"]');
      await page.click('[data-v2="run-cancel"][data-id="r-2"]');
      await page.waitForSelector("#run-cancel-reason");
      await page.fill("#run-cancel-reason", "superseded");
      await page.click('[data-v2="run-cancel-save"][data-id="r-2"]');
      await page.waitForSelector("text=Run cancelled");
      assert.equal(
        consoleState.runs.find((row) => row.id === "r-2").status,
        "cancelled",
        "the cancel crossed the wire",
      );

      // ── the portfolio is the tenant's real programme, not the fixture ────────────────
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#portfolio`);
      await page.waitForSelector('#console-chip[data-state="in"]');
      await page.waitForSelector('[data-v2="tab"][data-key="portfolio"][data-value="Dependencies"]');
      mainText = await page.locator("#main").innerText();
      assert.match(mainText, /conveyor buildout/, "the live programme row renders");
      assert.match(mainText, /S02/, "the portfolio lists the tenant's project");
      assert.match(mainText, /Skeleton/, "the portfolio names the real project");
      assert.doesNotMatch(
        mainText,
        /Identity & access|Citizen services/,
        "no fixture project is rendered signed in",
      );
      await page.click('[data-v2="tab"][data-key="portfolio"][data-value="Programme"]');
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("Programme queue"),
      );
      const queueText = await page.locator("#main").innerText();
      assert.match(queueText, /Programme queue/, "the programme queue renders");
      assert.match(queueText, /outputs due/, "the queue counts the due outputs");

      // ── #305: the programme definition and plan spine are the store's own rows ──────
      const mspSent = (name) =>
        consoleState.requests.filter((row) => row.path === `/api/actions/${name}/invocations`);
      const mspDone = () => page.waitForFunction(() => !document.getElementById("dialog").open);
      const mspTab = (key, value) =>
        page.click(`[data-v2="tab"][data-key="${key}"][data-value="${value}"]`);

      // Typed dependency edges live on Portfolio's Dependencies tab.
      await page.click('[data-v2="tab"][data-key="portfolio"][data-value="Dependencies"]');
      await page.waitForSelector('[data-v2="msp-dependency-new"]');
      assert.match(await page.locator("#main").innerText(), /blocks/, "the typed edge renders");
      await page.click('[data-v2="msp-dependency-new"]');
      await page.waitForSelector("#msp-to_project");
      await page.selectOption("#msp-to_project", "p-noboard");
      await page.selectOption("#msp-kind", "needs");
      await page.click('[data-v2="msp-save"][data-kind="dependency"]');
      await mspDone();
      assert.equal(mspSent("project_dependencies.create").length, 1, "project_dependencies.create was sent");
      await page.waitForSelector('[data-v2="msp-delete"][data-kind="dependency"][data-id="dep-2"]');
      await page.click('[data-v2="msp-delete"][data-kind="dependency"][data-id="dep-2"]');
      await page.waitForSelector('[data-v2="msp-delete-save"][data-kind="dependency"]');
      await page.click('[data-v2="msp-delete-save"]');
      await mspDone();
      assert.equal(mspSent("project_dependencies.delete").length, 1, "project_dependencies.delete was sent");
      assert.ok(!consoleState.msp.dependencies.some((row) => row.id === "dep-2"), "the stub deleted the edge");

      // Programme definition: five tabs read the store; each write is version-guarded.
      await page.click('[data-action="navigate"][data-page="programme"]');
      await page.waitForSelector('#console-chip[data-state="in"]');
      await page.waitForSelector('[data-v2="tab"][data-key="programme"][data-value="Definition"]');
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("conveyor buildout"),
      );
      for (const name of [
        "benefits.list",
        "benefits.link_list",
        "benefits.transition_list",
        "tranches.list",
        "tranches.review_list",
        "stakeholders.profile_list",
        "cadences.list",
        "wbs.list",
        "baselines.list",
        "registers.list",
        "project_dependencies.list",
        "portfolio.in_flight",
        "documents.list",
      ]) {
        assert.ok(mspSent(name).length > 0, `${name} was read for the programme pages`);
      }
      await page.click('[data-v2="msp-programme-edit"]');
      await page.waitForSelector("#msp-name");
      await page.fill("#msp-name", "conveyor buildout · console");
      await page.click('[data-v2="msp-save"][data-kind="programme"]');
      await mspDone();
      assert.equal(mspSent("programmes.update").length, 1, "programmes.update was sent");
      assert.equal(consoleState.msp.programme.name, "conveyor buildout · console", "the programme edit crossed the wire");

      await mspTab("programme", "Benefits map & profiles");
      await page.waitForSelector('[data-v2="msp-benefit"][data-id="ben-1"]');
      await page.click('[data-v2="msp-benefit-new"]');
      await page.waitForSelector("#msp-reference");
      await page.fill("#msp-reference", "BEN-99");
      await page.fill("#msp-description", "A new measured benefit");
      await page.click('[data-v2="msp-save"][data-kind="benefit"]');
      await mspDone();
      assert.equal(mspSent("benefits.create").length, 1, "benefits.create was sent");
      assert.ok(consoleState.msp.benefits.some((row) => row.reference === "BEN-99"), "the stub created the benefit");
      await page.click('[data-v2="msp-benefit-edit"][data-id="ben-1"]');
      await page.waitForSelector("#msp-description");
      await page.fill("#msp-description", "Streamline the intake flow");
      await page.click('[data-v2="msp-save"][data-kind="benefit"][data-id="ben-1"]');
      await mspDone();
      assert.equal(mspSent("benefits.update").length, 1, "benefits.update was sent");
      assert.equal(consoleState.msp.benefits.find((row) => row.id === "ben-1").description, "Streamline the intake flow");
      await page.click('[data-v2="msp-benefit"][data-id="ben-2"]');
      await page.waitForSelector('[data-v2="msp-delete"][data-kind="benefit"][data-id="ben-2"]');
      await page.click('[data-v2="msp-delete"][data-kind="benefit"][data-id="ben-2"]');
      await page.waitForSelector('[data-v2="msp-delete-save"][data-kind="benefit"]');
      await page.click('[data-v2="msp-delete-save"]');
      await mspDone();
      assert.equal(mspSent("benefits.delete").length, 1, "benefits.delete was sent");
      assert.ok(!consoleState.msp.benefits.some((row) => row.id === "ben-2"), "the stub deleted the benefit");
      await page.click('[data-v2="msp-benefit"][data-id="ben-1"]');
      await page.waitForSelector('[data-v2="msp-link-new"]');
      await page.click('[data-v2="msp-link-new"]');
      await page.waitForSelector("#msp-output_ref");
      await page.fill("#msp-output_ref", "T2 delivery spine");
      await page.fill("#msp-capability_ref", "delivery management");
      await page.fill("#msp-operational_outcome", "shorter lead time");
      await page.fill("#msp-strategic_objective", "faster IT delivery");
      await page.click('[data-v2="msp-save"][data-kind="link"]');
      await mspDone();
      assert.equal(mspSent("benefits.link").length, 1, "benefits.link was sent");
      await page.waitForSelector('[data-v2="msp-delete"][data-kind="link"][data-id="bl-1"]');
      await page.click('[data-v2="msp-delete"][data-kind="link"][data-id="bl-1"]');
      await page.waitForSelector('[data-v2="msp-delete-save"][data-kind="link"]');
      await page.click('[data-v2="msp-delete-save"]');
      await mspDone();
      assert.equal(mspSent("benefits.unlink").length, 1, "benefits.unlink was sent");
      assert.ok(!consoleState.msp.links.some((row) => row.id === "bl-1"), "the stub unlinked the edge");

      await mspTab("programme", "Benefits realization");
      await page.waitForSelector('[data-v2="msp-transition"][data-id="ben-1"]');
      await page.click('[data-v2="msp-transition"][data-id="ben-1"]');
      await page.waitForSelector("#msp-post_measured_at");
      await page.fill("#msp-post_measured_at", "2026-11-02");
      await page.click('[data-v2="msp-save"][data-kind="transition"][data-id="bt-1"]');
      await mspDone();
      assert.equal(mspSent("benefits.transition_upsert").length, 1, "benefits.transition_upsert was sent");
      assert.equal(
        consoleState.msp.transitions.find((row) => row.id === "bt-1").post_measured_at,
        "2026-11-02T00:00:00Z",
        "the post-transition measurement crossed the wire",
      );

      await mspTab("programme", "Tranches & reviews");
      await page.waitForSelector('[data-v2="msp-tranche-new"]');
      await page.click('[data-v2="msp-tranche-new"]');
      await page.waitForSelector("#msp-number");
      await page.fill("#msp-number", "3");
      await page.fill("#msp-entry_criteria", "tranche 2 reviewed");
      await page.click('[data-v2="msp-save"][data-kind="tranche"]');
      await mspDone();
      assert.equal(mspSent("tranches.create").length, 1, "tranches.create was sent");
      await page.click('[data-v2="msp-tranche-edit"][data-id="tr-1"]');
      await page.waitForSelector("#msp-exit_criteria");
      await page.fill("#msp-exit_criteria", "benefits measured against the baseline");
      await page.click('[data-v2="msp-save"][data-kind="tranche"][data-id="tr-1"]');
      await mspDone();
      assert.equal(mspSent("tranches.update").length, 1, "tranches.update was sent");
      await page.click('[data-v2="msp-review"][data-id="tr-1"]');
      await page.waitForSelector("#msp-decision");
      await page.selectOption("#msp-decision", "continue");
      await page.fill("#msp-baselines", "the scope baseline is frozen");
      await page.fill("#msp-benefit_status", "BEN-01 is tracking");
      await page.fill("#msp-risk_register", "no high risk is open");
      await page.click('[data-v2="msp-save"][data-kind="review"][data-id="tr-1"]');
      await mspDone();
      assert.equal(mspSent("tranches.review").length, 1, "tranches.review was sent");
      assert.equal(consoleState.msp.reviews.length, 2, "the review crossed the wire");
      await page.click('[data-v2="msp-tranche"][data-id="tr-2"]');
      await page.waitForSelector('[data-v2="msp-delete"][data-kind="tranche"][data-id="tr-2"]');
      await page.click('[data-v2="msp-delete"][data-kind="tranche"][data-id="tr-2"]');
      await page.waitForSelector('[data-v2="msp-delete-save"][data-kind="tranche"]');
      await page.click('[data-v2="msp-delete-save"]');
      await mspDone();
      assert.equal(mspSent("tranches.delete").length, 1, "tranches.delete was sent");

      await mspTab("programme", "Stakeholders");
      await page.waitForSelector('[data-v2="msp-stakeholder-new"]');
      await page.click('[data-v2="msp-stakeholder-new"]');
      await page.waitForSelector("#msp-name");
      await page.fill("#msp-name", "Delivery manager");
      await page.fill("#msp-importance", "high");
      await page.click('[data-v2="msp-save"][data-kind="stakeholder"]');
      await mspDone();
      assert.equal(mspSent("stakeholders.profile_create").length, 1, "stakeholders.profile_create was sent");
      await page.click('[data-v2="msp-stakeholder-edit"][data-id="sp-1"]');
      await page.waitForSelector("#msp-concerns");
      await page.fill("#msp-concerns", "value realised on schedule");
      await page.click('[data-v2="msp-save"][data-kind="stakeholder"][data-id="sp-1"]');
      await mspDone();
      assert.equal(mspSent("stakeholders.profile_update").length, 1, "stakeholders.profile_update was sent");
      await page.click('[data-v2="msp-stakeholder"][data-id="sp-2"]');
      await page.waitForSelector('[data-v2="msp-delete"][data-kind="stakeholder"][data-id="sp-2"]');
      await page.click('[data-v2="msp-delete"][data-kind="stakeholder"][data-id="sp-2"]');
      await page.waitForSelector('[data-v2="msp-delete-save"][data-kind="stakeholder"]');
      await page.click('[data-v2="msp-delete-save"]');
      await mspDone();
      assert.equal(mspSent("stakeholders.profile_delete").length, 1, "stakeholders.profile_delete was sent");

      // Plan & WBS: the WBS, baseline, register and cadence tabs are the project's own rows.
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#plan`);
      await page.waitForSelector('[data-v2="tab"][data-key="plan"][data-value="WBS"]');
      await page.click('[data-v2="tab"][data-key="plan"][data-value="WBS"]');
      await page.waitForSelector('[data-v2="msp-wbs-new"]');
      assert.match(await page.locator("#main").innerText(), /Skeleton phase/, "the WBS node renders");
      await page.click('[data-v2="msp-wbs-new"]');
      await page.waitForSelector("#msp-kind");
      await page.selectOption("#msp-kind", "work_package");
      await page.fill("#msp-title", "Tenancy work package");
      await page.click('[data-v2="msp-save"][data-kind="wbs"]');
      await mspDone();
      assert.equal(mspSent("wbs.create").length, 1, "wbs.create was sent");
      await page.click('[data-v2="msp-wbs-edit"][data-id="wn-1"]');
      await page.waitForSelector("#msp-title");
      await page.fill("#msp-title", "Skeleton phase · renamed");
      await page.click('[data-v2="msp-save"][data-kind="wbs"][data-id="wn-1"]');
      await mspDone();
      assert.equal(mspSent("wbs.update").length, 1, "wbs.update was sent");
      await page.click('[data-v2="msp-delete"][data-kind="wbs"][data-id="wn-2"]');
      await page.waitForSelector('[data-v2="msp-delete-save"][data-kind="wbs"]');
      await page.click('[data-v2="msp-delete-save"]');
      await mspDone();
      assert.equal(mspSent("wbs.delete").length, 1, "wbs.delete was sent");

      await page.click('[data-v2="tab"][data-key="plan"][data-value="Baseline vs forecast"]');
      await page.waitForSelector('[data-v2="msp-baseline-take"][data-kind="scope"]');
      assert.match(await page.locator("#main").innerText(), /bl-proj-1/, "the frozen baseline renders");
      await page.click('[data-v2="msp-baseline-take"][data-kind="scope"]');
      await page.waitForFunction(() =>
        document.querySelector("#toast").textContent.includes("Baseline taken"),
      );
      assert.equal(mspSent("baselines.take").length, 1, "baselines.take was sent");

      await page.click('[data-v2="tab"][data-key="plan"][data-value="Registers"]');
      await page.waitForSelector('[data-v2="msp-register-new"]');
      await page.click('[data-v2="msp-register-new"]');
      await page.waitForSelector("#msp-title");
      await page.selectOption("#msp-kind", "assumption");
      await page.fill("#msp-title", "The store refuses a referenced delete");
      await page.click('[data-v2="msp-save"][data-kind="register"]');
      await mspDone();
      assert.equal(mspSent("registers.create").length, 1, "registers.create was sent");
      await page.click('[data-v2="msp-register-edit"][data-id="ri-1"]');
      await page.waitForSelector("#msp-title");
      await page.fill("#msp-title", "Tenancy leak on the shared board · mitigated");
      await page.click('[data-v2="msp-save"][data-kind="register"][data-id="ri-1"]');
      await mspDone();
      assert.equal(mspSent("registers.update").length, 1, "registers.update was sent");
      await page.click('[data-v2="msp-delete"][data-kind="register"][data-id="ri-2"]');
      await page.waitForSelector('[data-v2="msp-delete-save"][data-kind="register"]');
      await page.click('[data-v2="msp-delete-save"]');
      await mspDone();
      assert.equal(mspSent("registers.delete").length, 1, "registers.delete was sent");

      await page.click('[data-v2="tab"][data-key="plan"][data-value="Cadence"]');
      await page.waitForSelector('[data-v2="msp-cadence-new"]');
      await page.click('[data-v2="msp-cadence-new"]');
      await page.waitForSelector("#msp-period");
      await page.selectOption("#msp-kind", "demo");
      await page.fill("#msp-period", "Demo 7");
      await page.click('[data-v2="msp-save"][data-kind="cadence"]');
      await mspDone();
      assert.equal(mspSent("cadences.create").length, 1, "cadences.create was sent");
      await page.click('[data-v2="msp-cadence-edit"][data-id="cd-1"]');
      await page.waitForSelector("#msp-period");
      await page.fill("#msp-period", "PI 2026.1 · extended");
      await page.click('[data-v2="msp-save"][data-kind="cadence"][data-id="cd-1"]');
      await mspDone();
      assert.equal(mspSent("cadences.update").length, 1, "cadences.update was sent");
      await page.click('[data-v2="msp-delete"][data-kind="cadence"][data-id="cd-2"]');
      await page.waitForSelector('[data-v2="msp-delete-save"][data-kind="cadence"]');
      await page.click('[data-v2="msp-delete-save"]');
      await mspDone();
      assert.equal(mspSent("cadences.delete").length, 1, "cadences.delete was sent");
      await page.click('[data-v2="tab"][data-key="plan"][data-value="Boards"]');
      await page.waitForSelector('[data-v2-input="plan-project"]');

      // back to the Plan board for the 412/401 honesty checks
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#plan`);
      await page.waitForSelector('[data-v2-input="plan-project"]');

      // 412 is rendered, not retried
      consoleState.failMove = 412;
      await page.waitForTimeout(150);
      await page.click('[data-v2="console-card"][data-id="c-1"]');
      await page.waitForSelector("#console-move");
      await page.click('[data-v2="console-move"][data-id="c-1"]');
      await page.waitForSelector("text=Move refused");
      assert.match(
        await page.locator("#main").innerText(),
        /Last action refused/,
        "the refusal renders on the page, not only as a toast",
      );
      consoleState.failMove = 0;

      // 401 clears the session and the page tells the truth
      await page.click('#dialog [data-action="close-dialog"]');
      await page.waitForFunction(() => !document.getElementById("dialog").open);
      consoleState.failProjects = 401;
      await page.click('[data-v2="console-reload"]');
      await page.waitForSelector('#console-chip[data-state="out"]');
      assert.match(await page.locator("#main").innerText(), /No published snapshot is loaded|Sign in to operate/);
      assert.equal(errors.length, 0, "no unhandled page error: " + errors.join("; "));
      await page.screenshot({ path: shot("console-signed-in.png"), fullPage: true });
      await page.close();
    }

    /* 2c — the Procedures destination: the intake, the source review and the two gates. */
    {
      const page = await browser.newPage();
      const errors = [];
      page.on("pageerror", (error) => errors.push(String(error)));
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#procedures`);
      await page.waitForSelector('#console-chip[data-state="in"]');
      await page.waitForSelector("text=Synthetic receiving SOP");
      assert.doesNotMatch(
        await page.locator("#main").innerText(),
        /pko/i,
        "the Procedures page claims no PKO store",
      );
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/procedures.list/invocations",
        ),
        "the page reads procedures.list",
      );
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/processes.list/invocations",
        ),
        "the page reads processes.list",
      );

      // The SOP intake: paste a document and ingest it. Nothing is published.
      await page.click('[data-v2="tab"][data-key="procedures"][data-value="SOP intake"]');
      await page.fill('[data-v2-input="proc-sop-title"]', "Synthetic receiving SOP");
      await page.fill('[data-v2-input="proc-sop-process-key"]', "receiving-stub");
      await page.fill('[data-v2-input="proc-sop-source-key"]', "receiving-sop-stub");
      await page.fill('[data-v2-input="proc-sop-text"]', "Receive stock, verify it, put it away.");
      await page.click('[data-v2="proc-ingest-sop"]');
      await page.waitForSelector("text=Draft created");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/procedures.ingest_sop/invocations",
        ),
        "procedures.ingest_sop was sent",
      );
      assert.match(
        await page.locator("#main").innerText(),
        /process_id/,
        "the ingest result names the draft",
      );

      // The PDDL steps read the derivation facts the P1 view carries.
      await page.click('[data-v2="tab"][data-key="procedures"][data-value="PDDL steps"]');
      await page.waitForSelector("text=verify_stock");
      const pddl = await page.locator("#main").innerText();
      assert.match(pddl, /put_away/, "the step action text renders");
      assert.match(pddl, /stock_verified/, "the PDDL precondition renders");

      await page.click('[data-v2="proc-validate"]');
      await page.waitForSelector("text=Every check passes");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/procedures.validate/invocations",
        ),
        "procedures.validate was sent",
      );

      // The source review is a human act and names the draft's own asset.
      await page.click('[data-v2="tab"][data-key="procedures"][data-value="Knowledge pairing"]');
      await page.waitForSelector('[data-v2-input="proc-review-note"]');
      await page.fill('[data-v2-input="proc-review-note"]', "the synthetic source is safe");
      await page.click('[data-v2="proc-review-approve"]');
      await page.waitForSelector("text=Source review");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/procedures.review_source/invocations",
        ),
        "procedures.review_source was sent",
      );

      // The executable gate: request, decide, publish — the approval rides the CALL.
      await page.click('[data-v2="tab"][data-key="procedures"][data-value="Publish"]');
      await page.waitForSelector('[data-v2="proc-gate-process"]');
      await page.click('[data-v2="proc-gate-process"]');
      await page.waitForSelector("text=Gate requested");
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/approvals.request/invocations",
        ),
        "approvals.request was sent",
      );
      // BOTH gates are requested before either is decided: the publish must select the
      // approved gate by its OWN name and target, never by the last request this session made.
      await page.click('[data-v2="proc-gate-procedure"]');
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("procedures.publish"),
      );
      assert.equal(
        consoleState.requests.filter(
          (row) => row.path === "/api/actions/approvals.request/invocations",
        ).length,
        2,
        "both gates were requested",
      );

      // A second person decides the executable gate; the process publish names gate-1 even
      // though the judge gate (gate-2) was requested last.
      await page.waitForSelector('[data-v2="proc-gate-approve"][data-id="gate-1"]');
      await page.click('[data-v2="proc-gate-approve"][data-id="gate-1"]');
      await page.waitForFunction(() => {
        const row = [...document.querySelectorAll("#main tr")].find((tr) =>
          tr.innerText.includes("processes.publish"),
        );
        return !!row && row.innerText.includes("approved");
      });
      await page.waitForTimeout(300);
      await page.click('[data-v2="proc-publish-process"]');
      await page.waitForTimeout(400);
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/processes.publish/invocations",
        ),
        "processes.publish was sent",
      );

      // The judge's own approval releases the judge.
      await page.waitForSelector('[data-v2="proc-gate-approve"][data-id="gate-2"]');
      await page.click('[data-v2="proc-gate-approve"][data-id="gate-2"]');
      await page.waitForTimeout(400);
      await page.click('[data-v2="proc-publish-procedure"]');
      await page.waitForTimeout(400);
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/procedures.publish/invocations",
        ),
        "procedures.publish was sent",
      );
      assert.deepEqual(errors, [], "no page error on the procedures destination");
      await page.screenshot({ path: shot("console-procedures.png"), fullPage: true });
      await page.close();
    }

    /* 2d — the programme refresh: the image's seed applied through the console. */
    {
      const page = await browser.newPage();
      const errors = [];
      page.on("pageerror", (error) => errors.push(String(error)));
      // The 401 case above leaves the stub refusing /api/projects; the programme page reads
      // them, and a 401 there (correctly) clears the session. Reset the fixture for this case.
      consoleState.failProjects = 0;
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#programme`);
      await page.waitForSelector('#console-chip[data-state="in"]');
      await page.waitForSelector('[data-v2="programmes-refresh"]');
      await page.click('[data-v2="programmes-refresh"]');
      await page.waitForFunction(() =>
        document.querySelector("#toast").innerText.includes("49 created"),
      );
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/programmes.refresh/invocations",
        ),
        "programmes.refresh was sent",
      );
      assert.deepEqual(errors, [], "no page error on the programme refresh");
      await page.close();
    }

    /* 2b — the architecture capability: the tenant's live rows, entries on selection, no writes. */
    {
      const page = await browser.newPage();
      const errors = [];
      page.on("pageerror", (error) => errors.push(String(error)));
      consoleState.failProjects = 0;
      await page.goto(`http://127.0.0.1:${signedIn.port}/studio.html#adm`);
      await page.waitForSelector('#console-chip[data-state="in"]');
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("Live tailored ADM profile"),
      );
      const liveAdm = await page.locator("#main").innerText();
      assert.match(liveAdm, /Live Architecture Board/, "the live board renders");
      assert.match(liveAdm, /Architecture governance/, "the live maturity baseline renders");
      assert.match(liveAdm, /API · no snapshot/, "the ribbon names the live source");
      for (const action of [
        "architecture.profile_get",
        "architecture.artifacts_list",
        "architecture.contracts_list",
        "architecture.board_list",
        "architecture.maturity_list",
        "architecture.skills_list",
      ]) {
        assert.ok(
          consoleState.requests.some(
            (row) => row.path === `/api/actions/${action}/invocations`,
          ),
          `${action} was read`,
        );
      }
      await page.click('[data-v2="tab"][data-value="Artefact repository"]');
      await page.waitForSelector('[data-v2="adm-artifact"][data-id="art-live-2"]');
      assert.match(
        await page.locator("#main").innerText(),
        /Live principles catalog/,
        "the repository lists the tenant's artefacts",
      );
      await page.click('[data-v2="adm-artifact"][data-id="art-live-2"]');
      await page.waitForFunction(() =>
        document.querySelector("#main").innerText.includes("Release management"),
      );
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/architecture.artifacts_entries/invocations",
        ),
        "selecting an artefact reads its entries",
      );
      assert.ok(
        consoleState.requests.some(
          (row) => row.path === "/api/actions/architecture.artifacts_links/invocations",
        ),
        "selecting an artefact reads its links",
      );
      assert.match(
        await page.locator("#main").innerText(),
        /derivation/,
        "the artefact's links render",
      );
      assert.equal(
        await page
          .locator(
            '[data-v2="adm-advance"], [data-v2="adm-approve"], [data-v2="adm-h-change"], [data-v2="adm-h-kata"], [data-v2="adm-review"], [data-v2="adm-waiver"]',
          )
          .count(),
        0,
        "the live capability carries no write control this unit",
      );
      assert.deepEqual(errors, [], "no page error on the live capability: " + errors.join("; "));
      await page.close();
    }

    console.log(
      "check-console: 4 servers · homepage + signed-out + snapshot 0 API requests · signed-in cards," +
        " projects, outputs, feedback, publish, runs, gates, programme definition, plan spine, portfolio and architecture",
    );
    console.log("check-console: 412 and 401 paths rendered; 0 unhandled page errors");
  } finally {
    await browser.close();
    homepageServer.server.close();
    signedOutServer.server.close();
    snapshotServer.server.close();
    closureServer.server.close();
    signedIn.server.close();
  }
})().catch((error) => {
  console.error("check-console: FAILED");
  console.error(error && error.stack ? error.stack : error);
  process.exit(1);
});
