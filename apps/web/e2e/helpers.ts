import { readFileSync } from "node:fs";

import { expect, type APIRequestContext, type Page } from "@playwright/test";

const TOKEN_FILE = process.env.E2E_TOKEN_FILE ?? "test-results/oidc-token.txt";
let oidc: string | undefined;

/**
 * The bearer the API calls use. Dev mode is the `dev:<login>` transport; the deployed suite is
 * a real PKCE login whose access token `oidc.setup.ts` captured from the app's own request —
 * the token itself stays memory-only in the browser.
 */
function oidcToken(): string {
  if (oidc === undefined) {
    oidc = readFileSync(TOKEN_FILE, "utf8").trim();
  }
  if (!oidc) {
    throw new Error(`no OIDC token at ${TOKEN_FILE}; did oidc.setup.ts run?`);
  }
  return oidc;
}

/** The bearer for one dev principal, or the captured OIDC token against a deployed app. */
export function authFor(login = "alice"): Record<string, string> {
  if (process.env.DEV_AUTH_TOKEN) {
    return { authorization: `Bearer dev:${login}` };
  }
  return { authorization: `Bearer ${oidcToken()}` };
}

/**
 * Whether this run is the dev transport, whose seed has a second dev-a admin (`dev:reviewer`).
 *
 * The deployed suite logs in as one OIDC principal, so a spec that needs a gate decided by
 * somebody other than its requester can only run here.
 */
export function isDevAuth(): boolean {
  return Boolean(process.env.DEV_AUTH_TOKEN);
}

/**
 * Invoke a real action. A failed call throws with the API's problem body, never a silent null.
 *
 * `approvalId` is what a destructive action needs (#282): the id travels BESIDE `input`, not
 * inside it, because every action input is `deny_unknown_fields`.
 */
export async function invoke<T = unknown>(
  request: APIRequestContext,
  name: string,
  input: unknown,
  ifMatch?: number,
  login = "alice",
  approvalId?: string,
): Promise<T> {
  const headers: Record<string, string> = { ...authFor(login) };
  if (ifMatch !== undefined) headers["if-match"] = String(ifMatch);
  const response = await request.post(`/api/actions/${name}/invocations`, {
    headers,
    data: approvalId ? { input, approval_id: approvalId } : { input },
  });
  if (!response.ok()) {
    throw new Error(`${name}: ${response.status()} ${await response.text()}`);
  }
  const document = await response.json();
  return document.output as T;
}

/** The reason a fixture records instead of typed evidence; it is not the gate's subject. */
export const FIXTURE_EVIDENCE_WAIVER =
  "e2e fixture: the gate's subject is the row the spec created";

/**
 * Record the tenant's decision for one action.
 *
 * §5.4 makes the requester's exclusion an invariant, so `self_approval` is sent as its only
 * storable spelling, `forbidden` (ledger v1-w4b-4). A spec that needs a gate states `required`
 * and decides it with [`decideAsReviewer`]; no spec may reintroduce the self-approval opt-out.
 */
export async function setApprovalPolicy(
  request: APIRequestContext,
  action: string,
  approval: "required" | "waived",
): Promise<void> {
  if (approval === "required") {
    await invoke(request, "approval_policies.set", {
      action,
      approval,
      self_approval: "forbidden",
    });
    return;
  }
  // An earned waiver is itself a gated change (§6.4, v1-w4b-3): open a gate whose target is the
  // governed action (a literal target, the shape a console fixture uses), have the distinct
  // reviewer decide it, then write the earned row with the approval id. A bare `waived` row is
  // refused and no longer skips the gate.
  const gate = await invoke<{ id: string }>(request, "approvals.request", {
    gate: "approval_policies.set",
    kind: "action",
    target: action,
    description: `e2e: an earned waiver for ${action}`,
    evidence_waiver_reason: FIXTURE_EVIDENCE_WAIVER,
  });
  await decideAsReviewer(request, "approvals.decide", gate.id);
  await invoke(
    request,
    "approval_policies.set",
    {
      action,
      approval: "waived",
      self_approval: "forbidden",
      approver_kind: "auto",
      conditions: { max_reverts: 0 },
      expires_at: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
    },
    undefined,
    "alice",
    gate.id,
  );
}

/**
 * Decide one gate as `dev:reviewer`, the seed's second dev-a admin.
 *
 * The store refuses a requester deciding their own gate unconditionally (§5.4 / P4); no policy
 * row can relax it, so the only way to resolve a gate is another person. Guard the caller with
 * [`isDevAuth`]: the deployed suite has one principal and nobody else can decide.
 */
export async function decideAsReviewer(
  request: APIRequestContext,
  action: string,
  approvalId: string,
): Promise<void> {
  await invoke(
    request,
    action,
    { approval_id: approvalId, decision: "approve" },
    undefined,
    "reviewer",
  );
}

/** The ids of the tenant's pending gates, as a snapshot to diff against. */
export async function pendingGateIds(request: APIRequestContext): Promise<Set<string>> {
  return new Set((await pendingGates(request)).map((gate) => gate.id));
}

/** The one gate that appeared since `before`. Matching the DIFFERENCE, not just the name,
 * keeps a suite that carries other fixtures' gates from deciding the wrong one. */
export async function newGate(
  request: APIRequestContext,
  before: Set<string>,
  gate: string,
  kind: string,
): Promise<PendingGate> {
  const opened = (await pendingGates(request)).find(
    (row) => !before.has(row.id) && row.gate === gate && row.kind === kind,
  );
  if (!opened) {
    throw new Error(`no new ${kind} gate on ${gate} is pending`);
  }
  return opened;
}

/**
 * Activate a job so its release may publish and a binding may pin it.
 *
 * A job is born a draft (v1-w1a-4), and the store refuses to publish a release or bind a draft
 * with `409 job_not_live`. `job_catalogue.update {status: "active"}` is what asks for the
 * activation; its effect is a `job_activation` gate a second principal decides.
 */
export async function activateJob(
  request: APIRequestContext,
  jobId: string,
  version = 1,
): Promise<void> {
  const before = await pendingGateIds(request);
  await invoke(request, "job_catalogue.update", { id: jobId, version, status: "active" });
  const activation = await newGate(request, before, "job_catalogue.update", "job_activation");
  await decideAsReviewer(request, "approvals.decide", activation.id);
}

/**
 * Create a job binding and activate it.
 *
 * A binding is never born live (v1-p44-2): `job_bindings.create` writes it `draft` and opens its
 * own `job_binding` activation gate, and a run that pins a draft binding is refused
 * `409 job_binding_stale` — the same code a genuinely stale release gets, so a fixture that
 * skips this step would pass a stale-pin assertion for the wrong reason.
 */
export async function createActiveBinding<T>(
  request: APIRequestContext,
  input: Record<string, unknown>,
): Promise<T> {
  const before = await pendingGateIds(request);
  const created = await invoke<T>(request, "job_bindings.create", input);
  const gate = await newGate(request, before, "job_bindings.create", "job_binding");
  await decideAsReviewer(request, "approvals.decide", gate.id);
  return created;
}

/**
 * Drive a UI write and prove it landed, re-driving it while it has not.
 *
 * The board screen recreates its forms on every refetch — an SSE resync does it right after a
 * board loads, and a fixture's own start run lands a coach card seconds later — and a click
 * dispatched inside that window is delivered to a node that no longer carries the handler, so
 * no request is posted. Measured 2026-11-27: `projects.create` answered 201 while no
 * `cards.create` ever reached the wire, and the same again on the label form. `settled` is a
 * read of the SERVER, never the DOM, so the caller keeps its own assertion; this only decides
 * whether to press the control again. It gives up after `attempts`, leaving the caller's
 * assertion to fail with its own evidence.
 */
export async function submitUntil(
  act: () => Promise<void>,
  settled: () => Promise<boolean>,
  attempts = 4,
  perAttemptMs = 8_000,
): Promise<void> {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    if (await settled()) return;
    await act();
    const deadline = Date.now() + perAttemptMs;
    while (Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 250));
      if (await settled()) return;
    }
  }
}

/**
 * Fixture path: request a gate for `action`, have a distinct approver decide it, then invoke
 * with its id.
 *
 * The fixture path for a spec that needs a destructive call to have happened (a cleanup, a
 * seeded row) without driving the console. A destructive gate requires evidence, and a fixture
 * is not the gate's subject, so the request records the same explicit `evidence_waiver_reason`
 * an operator's prompt records — never a silent bypass. §5.4 forbids the requester deciding it,
 * so the decision is [`decideAsReviewer`]'s. Guard the caller with [`isDevAuth`].
 */
export async function approvedInvoke<T = unknown>(
  request: APIRequestContext,
  name: string,
  input: unknown,
  login = "alice",
): Promise<T> {
  if (!isDevAuth()) {
    throw new Error(
      `${name}: a fixture gate needs a distinct approver; the deployed suite has one principal`,
    );
  }
  const gate = await invoke<{ id: string }>(
    request,
    "approvals.request",
    {
      gate: name,
      kind: "action",
      action: name,
      input,
      proposed_input: input,
      evidence_waiver_reason: FIXTURE_EVIDENCE_WAIVER,
    },
    undefined,
    login,
  );
  await decideAsReviewer(request, "approvals.decide", gate.id);
  return invoke<T>(request, name, input, undefined, login, gate.id);
}

export type PendingGate = {
  id: string;
  gate: string;
  kind?: string;
  status: string;
  target?: string;
};

/** The tenant's pending gates, newest first. */
export async function pendingGates(request: APIRequestContext): Promise<PendingGate[]> {
  const page = await invoke<{ items: PendingGate[] }>(request, "approvals.list", {
    status: "pending",
    limit: 50,
  });
  return page.items;
}

/** `cards.archive`'s target for one card — the string the invoke boundary compares. */
export function archiveTarget(cardId: string): string {
  return `card/${cardId}/archive`;
}

/** Poll until the pending gate for one target exists, and return it. */
export async function pendingGateFor(
  request: APIRequestContext,
  target: string,
): Promise<PendingGate> {
  let found: PendingGate | undefined;
  await expect
    .poll(
      async () => {
        found = (await pendingGates(request)).find((row) => row.target === target);
        return Boolean(found);
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  return found!;
}

/** The operator's answer to the console's §5.4 waiver prompt: a reason, or `null` to cancel. */
export type WaiverAnswer = string | null;

/**
 * Answer the console's §5.4 evidence-waiver prompt as an operator would.
 *
 * The app asks `window.prompt` only after `approvals.request` refuses `422
 * gate_evidence_missing`: Cancel (`null`) keeps the refusal and retries nothing, a non-empty
 * reason retries the same request once with `evidence_waiver_reason`, and a blank answer is
 * asked again. The last answer repeats if the app asks more times, so a bug cannot hang the
 * spec; `dialogs` is the number of prompts the app actually showed.
 */
export function answerWaiverPrompt(
  page: Page,
  answers: WaiverAnswer[],
): { dialogs: number } {
  const seen = { dialogs: 0 };
  let index = 0;
  page.on("dialog", (dialog) => {
    const answer = answers[Math.min(index, answers.length - 1)];
    index += 1;
    seen.dialogs += 1;
    void (answer === null ? dialog.dismiss() : dialog.accept(answer));
  });
  return seen;
}

/** One `approvals.request` input the page sent, in order. */
export type GateRequest = { input: Record<string, unknown> };

/** Every `approvals.request` input the page sends, in order. */
export function recordGateRequests(page: Page): GateRequest[] {
  const bodies: GateRequest[] = [];
  page.on("request", (request) => {
    if (
      request.method() === "POST" &&
      new URL(request.url()).pathname === "/api/actions/approvals.request/invocations"
    ) {
      bodies.push(request.postDataJSON() as GateRequest);
    }
  });
  return bodies;
}

export async function getJson<T = unknown>(
  request: APIRequestContext,
  path: string,
  login = "alice",
): Promise<T> {
  const response = await request.get(path, { headers: authFor(login) });
  if (!response.ok()) {
    throw new Error(`${path}: ${response.status()} ${await response.text()}`);
  }
  return response.json() as Promise<T>;
}

/**
 * Wait until the run the eval report screen will render is `complete`; return its id (#733).
 *
 * The screen renders `eval_runs.list`'s first row (`apps/web/src/screens/evals.rs:712`, newest
 * first). A run still `queued` has no report, so its tiles read "—" and the per-case table is
 * empty — and the page does not refetch by itself, so a DOM wait alone cannot recover. The
 * suite's earlier specs and the worker's drain are both slower under full-suite load, which is
 * how a report spec read a queued run. This is the explicit status wait the issue asks for: no
 * sleep, and the caller reloads afterwards as the screen read.
 */
export async function waitForReportRun(
  request: APIRequestContext,
  evalId: string,
): Promise<string> {
  let runId = "";
  await expect
    .poll(
      async () => {
        const page = await invoke<{ items: { id: string; status: string }[] }>(
          request,
          "eval_runs.list",
          { eval_id: evalId, limit: 50 },
        );
        const newest = page.items[0];
        runId = newest?.id ?? "";
        if (newest && ["void", "cancelled"].includes(newest.status)) {
          throw new Error(
            `the eval's newest run ${newest.id} is ${newest.status}: a void or cancelled run has ` +
              `no report, and it is terminal, so the screen this spec reads can never show one`,
          );
        }
        return newest?.status ?? "none";
      },
      { timeout: 60_000, intervals: [500, 1000, 2000, 3000] },
    )
    .toBe("complete");
  return runId;
}

export type Lane = { id: string; kind: string; name: string };

export type BoardFixture = {
  projectId: string;
  boardId: string;
  intake: Lane;
  active: Lane;
  card: { id: string; version: number; name: string };
};

/** A project, its template board, and one card in Intake — all through the real actions. */
export async function createBoardCard(
  request: APIRequestContext,
  title: string,
  login = "alice",
): Promise<BoardFixture> {
  // A project type is what makes a lane transition legal; without it `cards.move` refuses
  // with `card-move-not-allowed` ("the project declares no project_type"). The fixture must
  // exercise the real path, so it declares the seeded `delivery` type.
  const project = await invoke<{ id: string }>(
    request,
    "projects.create",
    {
      title,
      project_type_code: "delivery",
      // v1-e2e-1: a fixture is not programme work. The public pack excludes and lists it.
      test_row: true,
    },
    undefined,
    login,
  );
  const view = await getJson<{ boards: { id: string }[] }>(
    request,
    `/api/projects/${project.id}`,
    login,
  );
  const boardId = view.boards[0].id;
  const board = await getJson<{
    board: { id: string };
    lists: { list: Lane }[];
  }>(request, `/api/boards/${boardId}`, login);
  const intake = board.lists.find((lane) => lane.list.kind === "intake")!.list;
  const active = board.lists.find((lane) => lane.list.kind === "active")!.list;
  const card = await invoke<{ id: string; version: number; name: string }>(
    request,
    "cards.create",
    {
      board_id: board.board.id,
      list_id: intake.id,
      name: title,
      card_type_code: "bug",
    },
    undefined,
    login,
  );
  return { projectId: project.id, boardId, intake, active, card };
}
