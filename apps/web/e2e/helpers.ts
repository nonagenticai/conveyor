import { readFileSync } from "node:fs";

import { expect, type APIRequestContext } from "@playwright/test";

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

/**
 * This tenant lets a requester decide their own gate for `action` (#285's self-approval path).
 *
 * The console disables a requester's own Approve unless the action's policy says `allowed`, so
 * a spec that needs that control enabled sets the policy instead of borrowing a second
 * principal. `approval: required` is kept: the gate still exists and is still decided — that
 * is the console's flow, not a bypass of it.
 */
export async function allowSelfApproval(
  request: APIRequestContext,
  action: string,
  login = "alice",
): Promise<void> {
  await invoke(
    request,
    "approval_policies.set",
    { action, approval: "required", self_approval: "allowed" },
    undefined,
    login,
  );
}

/**
 * Decide one gate as `dev:reviewer`, the seed's second dev-a admin.
 *
 * The store refuses a requester deciding their own gate (409) unless the action's policy allows
 * it, and `approval_policies.set` only accepts registered destructive actions — so a card or
 * eval gate can only ever be decided by another person. Guard the caller with [`isDevAuth`]:
 * the deployed suite has one principal.
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

/**
 * Request a gate for `action`, approve it as the same principal, and invoke with its id.
 *
 * The fixture path for a spec that needs a destructive call to have happened (a cleanup, a
 * seeded row) without driving the console. Pair it with [`allowSelfApproval`].
 */
export async function approvedInvoke<T = unknown>(
  request: APIRequestContext,
  name: string,
  input: unknown,
  login = "alice",
): Promise<T> {
  const gate = await invoke<{ id: string }>(
    request,
    "approvals.request",
    {
      gate: name,
      kind: "action",
      action: name,
      input,
      proposed_input: input,
    },
    undefined,
    login,
  );
  await invoke(
    request,
    "approvals.decide",
    { approval_id: gate.id, decision: "approve" },
    undefined,
    login,
  );
  return invoke<T>(request, name, input, undefined, login, gate.id);
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
