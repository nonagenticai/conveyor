import { expect, test, type APIRequestContext, type Page } from "./fixture";

import { answerWaiverPrompt, FIXTURE_EVIDENCE_WAIVER, invoke, isDevAuth } from "./helpers";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice (admin of dev-a). */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

// T18-4 (#565) P3.1 — Operations. A destructive write without an approved gate must change
// nothing: the console asks the tenant's policy, requests a gate naming the exact target, and
// stops. The test sets the policy it is asserting (required / self-approval forbidden), drives
// the screen, and reads the gate list through the API — the refusal is the engine's, not the UI's.

type Gate = {
  id: string;
  gate: string;
  status: string;
  target?: string;
  evidence?: { kind: string; id: string }[];
};

async function setPolicy(
  request: APIRequestContext,
  action: string,
  approval: "required" | "waived",
  selfApproval: "forbidden" | "allowed",
): Promise<void> {
  await invoke(request, "approval_policies.set", {
    action,
    approval,
    self_approval: selfApproval,
  });
}

async function pendingGates(request: APIRequestContext): Promise<Gate[]> {
  const page = await invoke<{ items: Gate[] }>(request, "approvals.list", {
    status: "pending",
    limit: 50,
  });
  return page.items;
}

function notice(page: Page) {
  return page.getByTestId("operations-notice");
}

test("a destructive write needs the approvals", async ({ page, request }) => {
  await setPolicy(request, "ops.flag_flip", "required", "forbidden");
  // SPEC_V2 §5.4: a destructive gate refuses to open with no typed evidence. This fixture's
  // subject is the gate, not a document, so the console's bridge records the explicit waiver
  // reason and the gate opens — the write is still blocked behind it.
  answerWaiverPrompt(page, [FIXTURE_EVIDENCE_WAIVER]);
  const before = (await pendingGates(request)).filter((gate) => gate.gate === "ops.flag_flip");

  await page.goto("/operations");
  await expect(page.getByRole("heading", { name: "Operations" })).toBeVisible();
  await page.getByRole("tab", { name: "Flag changes" }).click();

  await page.getByLabel("Namespace").fill("conveyor");
  await page.getByLabel("Deployment").fill("conveyor");
  await page.getByLabel("Flag (environment variable)").fill("E2E_FLAG");
  await page.getByLabel("Value", { exact: true }).fill("true");
  await page.getByRole("button", { name: "Flip the flag" }).click();

  // The request is refused at the gate: the person is told what is waiting and that nothing
  // was written; the success sentence never appears.
  await expect(notice(page)).toContainText("Blocked: a gate");
  await expect(notice(page)).toContainText("Nothing was written.");
  await expect(notice(page)).not.toContainText("The flag was flipped");

  // The gate exists, names the flag action and its target — the write is waiting, not done.
  const after = (await pendingGates(request)).filter((gate) => gate.gate === "ops.flag_flip");
  expect(after.length).toBeGreaterThan(before.length);
  const opened = after.find((gate) => !before.some((old) => old.id === gate.id));
  expect(opened, "the attempt opened exactly one new gate").toBeTruthy();
  expect(opened!.target ?? "").toContain("E2E_FLAG=true");
});

// #572 — the operator attaches the gate's typed evidence in the destructive flow, the gate
// records what it resolved, and the approver reads it on the row. The evidence is the record the
// gate protects, not a free-text paraphrase.
test("a destructive gate records the evidence the operator attached", async ({ page, request }) => {
  await setPolicy(request, "ops.flag_flip", "required", "forbidden");

  // One approved governed asset, unique enough to search and rank first.
  const title = `Console evidence ${Date.now()}`;
  const asset = await invoke<{ id: string }>(request, "requirements.create", { title });
  await invoke(request, "requirements.transition", { id: asset.id, to: "in_review" });
  await invoke(request, "requirements.transition", { id: asset.id, to: "approved" });

  await page.goto("/operations");
  await expect(page.getByRole("heading", { name: "Operations" })).toBeVisible();
  await page.getByRole("tab", { name: "Flag changes" }).click();

  await page.getByLabel("Namespace").fill("conveyor");
  await page.getByLabel("Deployment").fill("conveyor");
  await page.getByLabel("Flag (environment variable)").fill("E2E_EVIDENCE");
  await page.getByLabel("Value", { exact: true }).fill("true");

  await page.getByLabel("Evidence search").fill(title);
  await page.getByRole("button", { name: "Find approved assets" }).click();
  const hit = page.getByTestId("evidence-hits").locator("li", { hasText: title });
  await expect(hit).toBeVisible();
  await hit.getByRole("button", { name: "Attach" }).click();
  await expect(page.getByTestId("evidence-attached")).toContainText(asset.id);

  const before = (await pendingGates(request)).filter((gate) => gate.gate === "ops.flag_flip");
  await page.getByRole("button", { name: "Flip the flag" }).click();
  await expect(notice(page)).toContainText("Blocked: a gate");

  // The gate carries the evidence the operator attached, resolved by the action.
  const after = (await pendingGates(request)).filter((gate) => gate.gate === "ops.flag_flip");
  const opened = after.find((gate) => !before.some((old) => old.id === gate.id));
  expect(opened, "the attempt opened exactly one new gate").toBeTruthy();
  expect(opened!.evidence).toEqual([{ kind: "governed_asset", id: asset.id }]);

  // The approver reads it on the row, beside the target.
  await page.goto("/approvals");
  const row = page.locator(`[data-approval="${opened!.id}"]`);
  await expect(row).toBeVisible();
  await expect(row.getByTestId("approval-evidence")).toContainText(asset.id);
});

test("the operations screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/operations");
  await expect(page.getByRole("heading", { name: "Operations" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});

// #848 — the P1.4 decision: Operations has no incident list by design (the screen "takes the
// ids it operates on rather than inventing a list"), so the honest empty state is the Andon
// tab's own Empty. `fr-test`'s tenants stay empty for the whole suite; nothing is inferred
// from test order.
test("an empty tenant sees the Andon tab's empty state, never a fixture", async ({ page }) => {
  test.skip(!isDevAuth(), "the deployed suite has one principal; fr-test is a dev seed");
  await as(page, "fr-test");
  await page.goto("/operations");
  await expect(page.getByRole("heading", { name: "Operations", exact: true })).toBeVisible();
  await page.getByRole("tab", { name: "Andon" }).click();
  const andon = page.getByTestId("operations-andon");
  await expect(andon.locator(".empty")).toContainText("No cord state read in this session");
});

// The cluster read is a closed operation answered by the estate integration. Nothing is
// configured in dev, so the server's own 503 is shown unchanged in the panel's problem state —
// that is Operations' error path, and the error is the server's, not a client guess.
test("a refused cluster read shows the server's own problem panel", async ({ page }) => {
  await page.goto("/operations");
  await page.getByRole("tab", { name: "Cluster & host" }).click();
  const cluster = page.getByTestId("operations-cluster");
  await cluster.getByLabel("Namespace").fill("conveyor");
  await cluster.getByRole("button", { name: "Read pods" }).click();
  const panel = cluster.locator("div.panel.problem[role=alert]");
  await expect(panel).toBeVisible();
  // The server's own sentence, not a loose keyword: ApiError renders `server 503: <detail>`.
  await expect(panel).toContainText('no base URL is configured for service "ops"');
});
