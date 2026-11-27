import { expect, test, type APIRequestContext } from "./fixture";

import {
  createBoardCard,
  decideAsReviewer,
  getJson,
  invoke,
  isDevAuth,
} from "./helpers";

// #566 T18-5 (W5) — the T12 acceptance from the UI. The backend loop is proven end to end by
// `apps/server/tests/organisation_loop.rs`; this spec proves the delivery-unit loop closes through
// the real actions and that each step's record is reachable from the destination that owns it.
//
// Two honest boundaries, both named rather than faked:
// - the gate decisions need a second principal, so the spec runs only under the dev transport
//   (`dev:reviewer`); the deployed suite has one principal;
// - the development project's own screen is the public site's unit record (#782), not an app
//   route yet: its closure is read back through `development_projects.get`, and the app screens
//   that hold the surrounding records (the board, the plan, the lesson) are asserted below.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

type Gate = { approval_id: string; auto_merge: boolean; status: string };
type Closure = {
  pr_url: string;
  merge_sha: string;
  deployed_digest: string;
  live_check: unknown;
  closure: {
    review: { reviewer_id: string };
    deploy: { digest: string; migration_generation: number };
  };
};

test("the delivery-unit loop closes with both digests and the live check", async ({
  page,
  request,
}) => {
  test.skip(!isDevAuth(), "the gates need a second principal; the dev seed carries dev:reviewer");
  const me = await getJson<{ principal: { id: string } }>(request, "/api/me");
  const title = unique("e2e acceptance unit");
  const created = await invoke<{ id: string }>(request, "development_projects.charter", {
    title,
    repo: "nonagenticai/conveyor",
    lane: `lane/e2e-acceptance-${Date.now()}`,
    issue_ref: "nonagenticai/conveyor#566",
    implementer_id: me.principal.id,
    acceptance_command: "true",
  });
  const id = created.id;

  await invoke(request, "development_projects.record_step", {
    development_project_id: id,
    step: "branch",
    evidence: { name: `lane/e2e-acceptance-${Date.now()}`, base: "main" },
  });
  await invoke(request, "development_projects.record_step", {
    development_project_id: id,
    step: "pull_request",
    evidence: {
      number: 1,
      url: "https://github.com/nonagenticai/conveyor/pull/1",
      head_sha: "abcdef1",
    },
  });
  await invoke(request, "development_projects.record_step", {
    development_project_id: id,
    step: "ci",
    evidence: { run_id: "e2e-run-1", sha: "abcdef1", verdict: "green" },
  });

  const review = await invoke<Gate>(request, "development_projects.review_gate", {
    development_project_id: id,
  });
  await decideAsReviewer(request, "approvals.decide", review.approval_id);

  const merge = await invoke<Gate>(request, "development_projects.merge_gate", {
    development_project_id: id,
  });
  if (!merge.auto_merge) {
    await decideAsReviewer(request, "approvals.decide", merge.approval_id);
  }

  const mergeSha = "a".repeat(40);
  await invoke(request, "development_projects.record_step", {
    development_project_id: id,
    step: "merge",
    evidence: { merge_sha: mergeSha },
  });
  await invoke(request, "development_projects.record_step", {
    development_project_id: id,
    step: "rollback_plan",
    evidence: {
      prior_digest: "sha256:prior",
      method: "Flux restores the pinned prior image",
      tested: true,
      backward_compatible: true,
      evidence: ["staging rollback exercised"],
    },
  });

  const deploy = await invoke<Gate>(request, "development_projects.deploy_gate", {
    development_project_id: id,
  });
  if (!deploy.auto_merge) {
    await decideAsReviewer(request, "approvals.decide", deploy.approval_id);
  }

  await invoke(request, "development_projects.record_step", {
    development_project_id: id,
    step: "deploy",
    evidence: { digest: "sha256:serving", migration_generation: 78 },
  });
  await invoke(request, "development_projects.record_step", {
    development_project_id: id,
    step: "live_verify",
    evidence: {
      health_status: 200,
      serving_digest: "sha256:serving",
      acceptance_command: "true",
      acceptance_exit: 0,
    },
  });

  const closed = await invoke<Closure>(request, "development_projects.close", {
    development_project_id: id,
  });
  expect(closed.pr_url).toContain("/pull/1");
  expect(closed.merge_sha).toBe(mergeSha);
  expect(closed.deployed_digest).toBe("sha256:serving");
  expect(closed.closure.deploy.migration_generation).toBe(78);
  expect(closed.closure.review.reviewer_id).toBeTruthy();

  // Read back: the closure is the record the client sees, never a client-side summary.
  const got = await invoke<{ closure: Closure }>(request, "development_projects.get", {
    development_project_id: id,
  });
  expect(got.closure.merge_sha).toBe(mergeSha);
});

test("the request, plan and lesson records render on their screens", async ({ page, request }) => {
  // 1. The request: a card on its board.
  const cardTitle = unique("e2e acceptance request");
  const fixture = await createBoardCard(request, cardTitle);
  await page.goto(`/board/${fixture.boardId}`);
  await expect(page.locator(".card", { hasText: cardTitle })).toBeVisible();

  // 2. The PM's plan: a project with a WBS and a baseline.
  const projectTitle = unique("e2e acceptance plan");
  const project = await invoke<{ id: string }>(request, "projects.create", {
    title: projectTitle,
    project_type_code: "delivery",
  });
  const wbsTitle = unique("e2e acceptance wbs");
  await invoke(request, "wbs.create", {
    project_id: project.id,
    kind: "deliverable",
    title: wbsTitle,
  });
  await invoke(request, "baselines.take", { project_id: project.id });
  await page.goto("/plan");
  const row = page
    .locator('[data-testid="plan-projects"] tr')
    .filter({ hasText: projectTitle })
    .first();
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "Open" }).click();
  await expect(page.locator('[data-testid="plan-wbs"]')).toContainText(wbsTitle);

  // 3. The lesson lifecycle: propose from the memory screen, then the independent review and
  // promotion through the action as the second principal. The screen cannot switch principal;
  // the T12 harness proves the multi-principal review, and the API is the same action the UI
  // would call.
  const second = await createBoardCard(request, unique("e2e acceptance evidence"));
  await page.goto("/knowledge/memory");
  await page.locator('label.field:has-text("Statement") input').fill(unique("e2e acceptance lesson"));
  await page.locator('label.field:has-text("Evidence 1") input').fill(fixture.card.id);
  await page.locator('label.field:has-text("Evidence 2") select').selectOption("card");
  await page.locator('label.field:has-text("Evidence 2") input').fill(second.card.id);
  await page.getByRole("button", { name: "Propose" }).click();
  await expect(page.locator('[data-testid="memory-lesson"]')).toContainText("proposed");
  const lessonId = (await page.locator('[data-testid="memory-lesson"] tbody tr td').first().textContent())!
    .trim();
  const reviewed = await invoke<{ state: string }>(
    request,
    "lessons.review",
    { lesson_id: lessonId, decision: "approve", note: "two independent refs" },
    undefined,
    "reviewer",
  );
  expect(reviewed.state).toBe("promoted");

  // 4. The other loop destinations render their own records.
  for (const [path, heading] of [
    ["/operations", "Operations"],
    ["/improve", "Improve"],
    ["/programme", "Programme"],
  ] as const) {
    await page.goto(path);
    await expect(page.getByRole("heading", { name: heading, level: 1 })).toBeVisible();
  }
});
