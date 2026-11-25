import { expect, test, type APIRequestContext, type Page } from "./fixture";

import { createBoardCard, getJson, invoke } from "./helpers";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice; bob is the empty tenant. */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

type MeetingFixture = { id: string; title: string };

async function seedMeeting(request: APIRequestContext, title: string): Promise<MeetingFixture> {
  const stamp = Date.now();
  const me = await getJson<{ principal: { id: string } }>(request, "/api/me");
  const board = await createBoardCard(request, title);
  const meeting = await invoke<{ id: string; card_name: string }>(request, "meetings.schedule", {
    title,
    list_id: board.intake.id,
    meeting_type_key: `board_${stamp}`,
    policy: {
      name: "Board",
      decision_rule: "majority",
      quorum: 1,
      agenda_sources: [],
      required_participation: [],
      evidence_minimum: 0,
    },
    convener_kind: "app_user",
    convener_id: me.principal.id,
  });
  const risk = await invoke<{ id: string }>(request, "registers.create", {
    project_id: board.projectId,
    kind: "risk",
    title: `${title} risk`,
  });
  await invoke(request, "meetings.agenda_add", {
    meeting_id: meeting.id,
    resolved_kind: "register_item",
    resolved_id: risk.id,
    evidence_refs: [],
  });
  await invoke(request, "meetings.invite", {
    meeting_id: meeting.id,
    participant_kind: "app_user",
    participant_id: me.principal.id,
    seat: "chair",
  });
  await invoke(request, "meetings.respond", {
    meeting_id: meeting.id,
    participant_kind: "app_user",
    participant_id: me.principal.id,
    response: "accepted",
  });
  await invoke(request, "meetings.open", { meeting_id: meeting.id });
  return { id: meeting.id, title };
}

test("the Meetings destination lists the forum and the room renders its record", async ({
  page,
  request,
}) => {
  const meeting = await seedMeeting(request, `Playwright meeting ${Date.now()}`);
  await page.goto("/meetings");
  await expect(page.getByRole("heading", { name: "Meetings" })).toBeVisible();
  await expect(page.getByRole("link", { name: meeting.title })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);

  await page.goto(`/meetings/${meeting.id}`);
  await expect(page.getByRole("heading", { name: meeting.title })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Agenda" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Participants" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Positions" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Outcome instruments" })).toBeVisible();
  await expect(page.getByText("chair", { exact: false })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the destination lists the forum policies", async ({ page, request }) => {
  await seedMeeting(request, `Playwright policy list ${Date.now()}`);
  await page.goto("/meetings");
  await expect(page.getByRole("heading", { name: "Forum policies" })).toBeVisible();
  await expect(page.getByText("board_", { exact: false }).first()).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the owner seat is offered and an unseated principal sees the refusal", async ({
  page,
  request,
}) => {
  const meeting = await seedMeeting(request, `Playwright owner seat ${Date.now()}`);
  await page.goto(`/meetings/${meeting.id}`);
  await expect(page.getByRole("button", { name: "Approve the first item" })).toBeVisible();

  // dev-b is a different tenant: the meeting is not theirs, and the screen says so rather than
  // rendering an empty room.
  await as(page, "bob");
  await page.goto(`/meetings/${meeting.id}`);
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".problem")).toContainText("not found");
  await page.goto("/meetings");
  await expect(page.getByText("No meeting in this tenant yet.")).toBeVisible();
});

test("the empty destination is honest", async ({ page }) => {
  await as(page, "bob");
  await page.goto("/meetings");
  await expect(page.getByRole("heading", { name: "Meetings" })).toBeVisible();
  await expect(page.getByText("No meeting in this tenant yet.")).toBeVisible();
  await expect(page.getByText("No forum policy in this tenant yet.")).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the room does not overflow a phone or a tablet", async ({ page, request }) => {
  const meeting = await seedMeeting(request, `Playwright meeting layout ${Date.now()}`);
  for (const width of [1024, 390]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto(`/meetings/${meeting.id}`);
    await expect(page.getByRole("heading", { name: meeting.title })).toBeVisible();
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
    );
    expect(overflow, `horizontal overflow at ${width}px`).toBe(false);
  }
});
