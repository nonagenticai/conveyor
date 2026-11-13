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
        meta: { issue: 2, unit_key: "S02" },
        version: 1,
        created_at: "2026-11-13T00:00:00Z",
        updated_at: "2026-11-13T00:00:00Z",
      },
    ],
    comments: {},
    requests: [],
    failProjects: 0,
    failMove: 0,
  };
}

function json(res, status, body, headers = {}) {
  res.writeHead(status, Object.assign({ "Content-Type": "application/json; charset=utf-8" }, headers));
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

function startServer({ config, state }) {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, "http://127.0.0.1");
    const pathname = decodeURIComponent(url.pathname);
    if (pathname === "/config.json") {
      if (!config) return json(res, 404, { title: "Not Found" });
      return json(res, 200, config);
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
        return json(res, 200, { items: [PROJECT], next_cursor: null });
      }
      if (pathname === "/api/projects/p-s02" && req.method === "GET") {
        return json(res, 200, { project: PROJECT, boards: [{ id: "b-s02", project_id: "p-s02", name: "Board", version: 1 }] });
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
          assert.equal(input.meta && input.meta.issue, 3, "cards.create carries the issue link");
          assert.equal(input.card_type_code, "architecture_task", "cards.create sends the card type");
          state.cards.push({
            id: "c-2",
            board_id: "b-s02",
            list_id: input.list_id,
            card_type_code: input.card_type_code,
            name: input.name,
            description: "",
            position: 2,
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
          json(res, 201, { id: "c-2", name: input.name, version: 1 });
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
      return json(res, 404, { title: "Not Found", detail: `no stub route for ${pathname}` });
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

      // 412 is rendered, not retried
      await page.click('#dialog [data-action="close-dialog"]');
      await page.waitForFunction(() => !document.getElementById("dialog").open);
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

    console.log("check-console: 3 servers · homepage + signed-out 0 API requests · signed-in intake/move/comment/claim/release");
    console.log("check-console: 412 and 401 paths rendered; 0 unhandled page errors");
  } finally {
    await browser.close();
    homepageServer.server.close();
    signedOutServer.server.close();
    signedIn.server.close();
  }
})().catch((error) => {
  console.error("check-console: FAILED");
  console.error(error && error.stack ? error.stack : error);
  process.exit(1);
});
