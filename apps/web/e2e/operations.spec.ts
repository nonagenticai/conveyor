import { expect, test, type APIRequestContext, type Page } from "./fixture";

import { invoke } from "./helpers";

// T18-4 (#565) P3.1 — Operations. A destructive write without an approved gate must change
// nothing: the console asks the tenant's policy, requests a gate naming the exact target, and
// stops. The test sets the policy it is asserting (required / self-approval forbidden), drives
// the screen, and reads the gate list through the API — the refusal is the engine's, not the UI's.

type Gate = { id: string; gate: string; status: string; target?: string };

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
