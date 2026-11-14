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
  meta: { issues: [2, 3], plan: "docs/plans/slices/S02-skeleton.md", track: "v1" },
  version: 1,
};

function freshState() {
  return {
    project: JSON.parse(JSON.stringify(PROJECT)),
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
    requests: [],
    failProjects: 0,
    failMove: 0,
    failDocument: 0,
  };
}

// A small real-shaped snapshot for the public-pack shape: two projects, three issues, one board.
const SNAPSHOT = {
  schema: "design-snapshot/v1",
  generated_at: "2026-11-14T00:00:00Z",
  git_sha: "0".repeat(40),
  payload_sha256: "a".repeat(64),
  tenant: "nonagentic",
  feedback_url: "https://github.com/nonagenticai/conveyor/discussions",
  programme: {
    programme: [{ id: "prog-1", name: "conveyor buildout", status: "active" }],
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
  work: {
    boards: [{ id: "b-s02", project_id: "p-s02" }],
    lists: [{ id: "l-done", board_id: "b-s02", kind: "done" }],
    cards: [{ id: "c-1", board_id: "b-s02", list_id: "l-done" }],
    runs: [],
  },
  config: {},
  organisation: {},
  artefacts: {},
  measure: {},
};

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

function startServer({ config, state, snapshot }) {
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
          "cards.create",
          "cards.move",
          "cards.comment",
          "cards.claim",
          "cards.release",
          "catalogues.list",
          "approvals.decide",
          "approvals.list",
          "process_runs.start",
          "programmes.list",
          "runs.cancel",
          "runs.get",
          "runs.list",
          "documents.compose",
          "documents.get",
          "documents.list",
          "documents.update",
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
        return json(res, 200, { items: [state.project], next_cursor: null });
      }
      if (pathname === "/api/projects/p-s02" && req.method === "GET") {
        return json(res, 200, { project: state.project, boards: [{ id: "b-s02", project_id: "p-s02", name: "Board", version: 1 }] });
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
          const approval = state.approvals.find((row) => row.id === input.approval_id);
          assert.ok(approval, "approvals.decide names a real gate");
          assert.ok(["approve", "reject"].includes(input.decision), "a valid decision");
          assert.ok(input.note, "the decision carries a note");
          approval.status = input.decision === "approve" ? "approved" : "rejected";
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
      if (pathname === "/api/actions/projects.update/invocations") {
        return readBody(req, (body) => {
          try {
            const input = body.input || {};
            assert.equal(input.id, "p-s02", "projects.update names the project");
            assert.equal(req.headers["if-match"], `"${state.project.version}"`, "projects.update carries If-Match");
            for (const key of ["name", "description", "status", "repo_url"]) {
              if (input[key] !== undefined) state.project[key] = input[key];
            }
            if (input.meta) state.project.meta = Object.assign({}, state.project.meta, input.meta);
            state.project.version += 1;
            json(res, 200, {
              project: state.project,
              boards: [{ id: "b-s02", project_id: "p-s02", name: "Board", version: 1 }],
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
          assert.equal(body.input.project_id, "p-s02", "documents.list is scoped to the project");
          const items = state.documents.map((doc) => {
            const summary = Object.assign({}, doc);
            delete summary.body;
            return summary;
          });
          json(res, 200, { items, next_cursor: null });
        });
        return;
      }
      if (pathname === "/api/actions/documents.compose/invocations") {
        return readBody(req, (body) => {
          const input = body.input || {};
          assert.equal(input.project_id, "p-s02", "documents.compose is scoped to the project");
          assert.ok(input.title && input.body, "documents.compose sends a title and a body");
          const doc = {
            id: `d-${state.documents.length + 1}`,
            project_id: input.project_id,
            card_id: input.card_id || null,
            kind: "document",
            title: input.title,
            status: "active",
            content_type: "text/markdown",
            size_bytes: input.body.length,
            content_hash: "a".repeat(64),
            body: input.body,
            external_ref: null,
            meta: {},
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
      if (pathname === "/api/actions/documents.get/invocations") {
        return readBody(req, (body) => {
          const doc = state.documents.find((row) => row.id === body.input.id);
          if (!doc) return json(res, 404, { title: "Not Found", detail: "no such document" });
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
          if (body.input.publishable !== undefined) doc.publishable = body.input.publishable;
          doc.version += 1;
          json(res, 200, doc);
        });
        return;
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
    if (pathname === "/data/snapshot.js" && snapshot) {
      res.writeHead(200, { "Content-Type": MIME[".js"] });
      return res.end("window.CONVEYOR_SNAPSHOT = " + JSON.stringify(snapshot) + ";");
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
      assert.match(await page.locator("#main").innerText(), /Work breakdown/, "file:// keeps the WBS default");
      await page.click('[data-v2="tab"][data-key="plan"][data-value="Boards"]');
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
      await page.close();
    }

    /* 1c — the public pack with a snapshot: real rows, zero API, contribute units. */
    {
      const page = await browser.newPage();
      await page.goto(`http://127.0.0.1:${snapshotServer.port}/studio.html#portfolio`);
      await page.waitForSelector(".page");
      await page.waitForTimeout(300);
      const portfolioText = await page.locator("#main").innerText();
      assert.match(portfolioText, /S02/, "the live portfolio renders the snapshot's project");
      assert.match(portfolioText, /Skeleton/, "the real project name renders");
      assert.doesNotMatch(
        portfolioText,
        /Identity & access|Citizen services/,
        "no fixture project on the live portfolio",
      );
      await page.goto(`http://127.0.0.1:${snapshotServer.port}/studio.html#contribute`);
      await page.waitForSelector(".page");
      await page.waitForTimeout(200);
      const contributeText = await page.locator("#main").innerText();
      assert.match(contributeText, /Ready to claim/, "contribute renders the ready summary");
      assert.match(contributeText, /#2/, "a ready unit links its issue");
      assert.match(contributeText, /Unassigned/, "an unlinked ready unit is grouped honestly");
      assert.match(contributeText, /How a unit is claimed/, "the claim protocol renders");
      assert.equal(snapshotState.requests.length, 0, "the public pack makes no API request");
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

      await page.click('[data-v2="project-edit"][data-id="p-s02"]');
      await page.waitForSelector("#project-name");
      await page.fill("#project-name", "Skeleton — edited in the console");
      await page.click('[data-v2="project-save"][data-id="p-s02"]');
      await page.waitForSelector("text=Project updated");
      assert.ok(
        consoleState.requests.some((row) => row.path === "/api/actions/projects.update/invocations"),
        "projects.update was sent",
      );
      assert.equal(
        consoleState.project.name,
        "Skeleton — edited in the console",
        "the stub applied the edit",
      );

      await page.waitForSelector('[data-v2="output-new"][data-project="p-s02"]');
      await page.click('[data-v2="output-new"][data-project="p-s02"]');
      await page.waitForSelector("#output-title");
      await page.fill("#output-title", "S02 design note");
      await page.fill("#output-body", "# Design note\n\nA decision recorded from the console.");
      await page.check("#output-publishable");
      await page.click('[data-v2="output-save"][data-project="p-s02"]');
      await page.waitForSelector("text=Output saved and marked public");
      assert.ok(
        consoleState.requests.some((row) => row.path === "/api/actions/documents.compose/invocations"),
        "documents.compose was sent",
      );
      await page.waitForSelector('[data-v2="output-read"][data-id="d-1"]');
      assert.match(await page.locator("#main").innerText(), /S02 design note/, "the output is listed");

      await page.click('[data-v2="output-publish"][data-id="d-1"]');
      await page.waitForSelector("text=Output unpublished");
      assert.ok(
        consoleState.requests.some((row) => row.path === "/api/actions/documents.update/invocations"),
        "documents.update was sent",
      );
      assert.equal(consoleState.documents[0].publishable, false, "the publish flag crossed the wire");

      await page.click('[data-v2="feedback-intake"][data-issue="9"]');
      await page.waitForSelector("text=Issue #9 intaken as a card");
      assert.ok(
        consoleState.cards.some((card) => card.meta && card.meta.issue === 9),
        "the feedback issue became a card",
      );

      assert.ok(
        (await page.locator('button[disabled]:has-text("Publish the design site")').count()) >= 1,
        "publish is disabled without a target",
      );
      await page.click("summary:has-text('Connect a homepage repository')");
      await page.fill("#publish-token", "ghp_test_token");
      await page.click('[data-v2="publish-connect"]');
      await page.waitForSelector("text=Publish target connected");
      assert.ok(
        consoleState.requests.some((row) => row.path === "/api/actions/repositories.create/invocations"),
        "repositories.create was sent",
      );
      await page.waitForSelector('[data-v2="publish-start"]');
      await page.click('[data-v2="publish-start"]');
      await page.waitForSelector("text=Publish enqueued");
      await page.waitForSelector(
        'a[href="https://github.com/nonagenticai/nonagentic_homepage/pull/7"]',
      );
      assert.ok(
        consoleState.requests.some((row) => row.path === "/api/actions/publish.start/invocations"),
        "publish.start was sent",
      );
      assert.equal(
        consoleState.publish.pr_url,
        "https://github.com/nonagenticai/nonagentic_homepage/pull/7",
        "the stub reports the PR",
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

    console.log(
      "check-console: 4 servers · homepage + signed-out + snapshot 0 API requests · signed-in cards," +
        " projects, outputs, feedback, publish, runs, gates and portfolio",
    );
    console.log("check-console: 412 and 401 paths rendered; 0 unhandled page errors");
  } finally {
    await browser.close();
    homepageServer.server.close();
    signedOutServer.server.close();
    snapshotServer.server.close();
    signedIn.server.close();
  }
})().catch((error) => {
  console.error("check-console: FAILED");
  console.error(error && error.stack ? error.stack : error);
  process.exit(1);
});
