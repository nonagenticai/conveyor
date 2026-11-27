import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { writeFileSync } from "node:fs";

// The W1 spike's end-to-end evidence. It creates its fixtures through the real actions, drives
// the spike's exported proofs, measures both renderers, and writes RESULTS.json for the record.
test.describe.configure({ mode: "serial" });

const results: Record<string, unknown> = {};
const RESULTS_FILE = process.env.RESULTS_FILE ?? "spike/walk/results.json";
const TOKEN = "dev:alice";

type Fixture = {
  projectId: string;
  boardId: string;
  intakeListId: string;
  activeListId: string;
  cardId: string;
  version: number;
};

async function invoke(
  request: APIRequestContext,
  name: string,
  input: unknown,
  ifMatch?: number,
): Promise<any> {
  const headers: Record<string, string> = { authorization: `Bearer ${TOKEN}` };
  if (ifMatch !== undefined) headers["if-match"] = String(ifMatch);
  const response = await request.post(`/api/actions/${name}/invocations`, {
    headers,
    data: { input },
  });
  if (!response.ok()) throw new Error(`${name}: ${response.status()} ${await response.text()}`);
  return (await response.json()).output;
}

async function getJson(request: APIRequestContext, path: string): Promise<any> {
  const response = await request.get(path, { headers: { authorization: `Bearer ${TOKEN}` } });
  if (!response.ok()) throw new Error(`${path}: ${response.status()} ${await response.text()}`);
  return response.json();
}

async function makeFixture(request: APIRequestContext): Promise<Fixture> {
  const title = `spike ${Date.now()}`;
  const project = await invoke(request, "projects.create", {
    title,
    project_type_code: "delivery",
  });
  const view = await getJson(request, `/api/projects/${project.id}`);
  const boardId = view.boards[0].id;
  const board = await getJson(request, `/api/boards/${boardId}`);
  const intake = board.lists.find((entry: any) => entry.list.kind === "intake").list;
  const active = board.lists.find((entry: any) => entry.list.kind === "active").list;
  const card = await invoke(request, "cards.create", {
    board_id: boardId,
    list_id: intake.id,
    name: `spike card ${Date.now()}`,
    card_type_code: "bug",
  });
  return {
    projectId: project.id,
    boardId,
    intakeListId: intake.id,
    activeListId: active.id,
    cardId: card.id,
    version: card.version,
  };
}

async function throttle(page: Page): Promise<void> {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Emulation.setCPUThrottlingRate", { rate: 4 });
  await cdp.send("Network.enable");
  await cdp.send("Network.emulateNetworkConditions", {
    offline: false,
    latency: 150,
    downloadThroughput: (1.5 * 1024 * 1024) / 8,
    uploadThroughput: (750 * 1024) / 8,
  });
  await cdp.send("Network.clearBrowserCache");
}

function stats(values: Array<number | null>): { min: number; median: number; max: number } | null {
  const sorted = values.filter((value): value is number => typeof value === "number").sort((a, b) => a - b);
  if (sorted.length === 0) return null;
  return {
    min: sorted[0],
    median: sorted[Math.floor(sorted.length / 2)],
    max: sorted[sorted.length - 1],
  };
}

function summarize(runs: any[]): Record<string, unknown> {
  return {
    available: runs.every((run) => run.available),
    runs: runs.length,
    p50: stats(runs.map((run) => run.p50)),
    p95: stats(runs.map((run) => run.p95)),
    longTasks: stats(runs.map((run) => run.longTasks?.count ?? 0)),
    heapAfter: stats(runs.map((run) => run.heapAfter ?? null)),
    coldLoad: stats(runs.map((run) => run.coldLoad ?? null)),
    bytes: runs[0]?.bytes ?? null,
    requests: runs[0]?.requests ?? null,
  };
}

test.afterAll(() => {
  writeFileSync(RESULTS_FILE, `${JSON.stringify(results, null, 2)}\n`);
});

test("the client contracts work end to end", async ({ page, request }) => {
  const fixture = await makeFixture(request);
  await page.goto("/walk/");
  await expect(page).toHaveTitle(/Conveyor Walk spike/);
  await expect(page.locator("#status")).toContainText("signed in");
  await page.evaluate((f) => (window as any).__spike.configure(f), fixture);

  const me = await page.evaluate(() => (window as any).__spike.me());
  expect(me?.principal?.display_name).toBeTruthy();

  const pkce = await page.evaluate(() =>
    (window as any).__spike.pkce({
      issuer: "https://issuer.example/",
      client_id: "walk",
      redirect_uri: "http://127.0.0.1:8932/walk/",
    }),
  );
  expect(pkce.ok).toBe(true);
  const authorize = new URL(pkce.url);
  expect(authorize.pathname).toBe("/protocol/openid-connect/auth");
  expect(authorize.searchParams.get("code_challenge_method")).toBe("S256");
  expect(authorize.searchParams.get("redirect_uri")).toBe("http://127.0.0.1:8932/walk/");
  expect(pkce.verifier.length).toBeGreaterThanOrEqual(43);

  const config = await page.evaluate(() => (window as any).__spike.config());
  const forms = await page.evaluate(() => (window as any).__spike.forms());
  expect(forms["programmes.list"].effect).toBe("read");
  expect(forms["programmes.update"].effect).toBe("write");
  expect(forms["cards.archive"].effect).toBe("destructive");
  expect(forms["cards.archive"].fields).toContain("id");

  const read = await page.evaluate(() => (window as any).__spike.read());
  expect(Array.isArray(read.items)).toBe(true);

  const created = await page.evaluate(
    (name) => (window as any).__spike.createCard(name),
    `spike create ${Date.now()}`,
  );
  expect(created.card.id).toBeTruthy();
  const createTrace = await page.evaluate(() =>
    (window as any).__spike.trace.find((entry: any) => entry.path.includes("cards.create")),
  );
  expect(createTrace.headers["idempotency-key"]).toBeTruthy();

  // The destructive gate: the policy requires an approval and this call names none.
  await invoke(request, "approval_policies.set", {
    action: "cards.archive",
    approval: "required",
    self_approval: "forbidden",
  });
  const refused = await page.evaluate(() => (window as any).__spike.archive());
  expect(refused.ok).toBe(false);
  expect(refused.status).toBe(403);
  expect(JSON.stringify(refused.problem)).toContain("approval");

  // SSE over fetch, then a real move on the board topic.
  await page.evaluate(() => (window as any).__spike.subscribe());
  const moved = await page.evaluate(() => (window as any).__spike.moveCard());
  expect(moved.card.id).toBe(fixture.cardId);
  const moveTrace = await page.evaluate(() =>
    (window as any).__spike.trace.find((entry: any) => entry.path.includes("cards.move")),
  );
  expect(moveTrace.headers["if-match"]).toBe(String(fixture.version));
  await page.waitForFunction(
    () => (window as any).__spike.sseFrames().length > 0,
    undefined,
    { timeout: 20_000 },
  );
  const frames = await page.evaluate(() => (window as any).__spike.sseFrames());
  expect(
    frames.some(
      (frame: any) => frame.event === "card.moved" || frame.data?.type === "card.moved",
    ),
  ).toBe(true);

  // The scoped fallback: a deep link returns the walk entry, not a different app.
  await page.goto("/walk/portfolio/deep-link");
  await expect(page).toHaveTitle(/Conveyor Walk spike/);
  expect(page.url()).toContain("/walk/portfolio/deep-link");

  results.proofs = {
    me: { principal: me.principal, active_tenant: me.active_tenant },
    pkce: {
      ok: pkce.ok,
      url: pkce.url,
      redirectUri: pkce.redirectUri,
      challengeMethod: authorize.searchParams.get("code_challenge_method"),
      verifierLength: pkce.verifier.length,
    },
    config,
    forms,
    readItems: read.items.length,
    createdCard: created.card.id,
    createHeaders: createTrace.headers,
    moveHeaders: moveTrace.headers,
    refusal: { status: refused.status, problem: refused.problem },
    sseFrames: frames.length,
    sseTypes: frames.map((frame: any) => frame.event ?? frame.data?.type ?? "frame"),
    deepLink: page.url(),
  };
});

test("canvas-2D measurement", async ({ page }) => {
  const runs: any[] = [];
  for (let index = 0; index < 5; index += 1) {
    await throttle(page);
    await page.goto(`/walk/?renderer=canvas2d&run=${index}`);
    await page.waitForFunction(() => Boolean((window as any).__spike));
    runs.push(await page.evaluate(() => (window as any).__spike.measure("canvas2d", 5)));
  }
  results.canvas2d = summarize(runs);
});

test("WebGL2 measurement", async ({ page }) => {
  const runs: any[] = [];
  for (let index = 0; index < 5; index += 1) {
    await throttle(page);
    await page.goto(`/walk/?renderer=webgl2&run=${index}`);
    await page.waitForFunction(() => Boolean((window as any).__spike));
    runs.push(await page.evaluate(() => (window as any).__spike.measure("webgl2", 5)));
  }
  results.webgl2 = summarize(runs);
});

test("the five-minute heap soak", async ({ page }) => {
  await page.goto("/walk/?renderer=canvas2d&soak=1");
  await page.waitForFunction(() => Boolean((window as any).__spike));
  results.soak = await page.evaluate(() => (window as any).__spike.soak("canvas2d"));
});

test("environment", async ({ browser }) => {
  results.environment = {
    browser: browser.version(),
    platform: process.platform,
    runner: process.env.RUNNER_NAME ?? null,
    cpuThrottle: 4,
    network: "Slow 4G emulation (1.5 Mbps down / 750 kbps up / 150 ms)",
    referenceScene: "6 rooms, 12 figures, labels, one moving figure (SPEC §12)",
  };
});
