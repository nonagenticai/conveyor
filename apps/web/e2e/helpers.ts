import { readFileSync } from "node:fs";

import type { APIRequestContext } from "@playwright/test";

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

/** Invoke a real action. A failed call throws with the API's problem body, never a silent null. */
export async function invoke<T = unknown>(
  request: APIRequestContext,
  name: string,
  input: unknown,
  ifMatch?: number,
  login = "alice",
): Promise<T> {
  const headers: Record<string, string> = { ...authFor(login) };
  if (ifMatch !== undefined) headers["if-match"] = String(ifMatch);
  const response = await request.post(`/api/actions/${name}/invocations`, {
    headers,
    data: { input },
  });
  if (!response.ok()) {
    throw new Error(`${name}: ${response.status()} ${await response.text()}`);
  }
  const document = await response.json();
  return document.output as T;
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
