import { expect, test } from "@playwright/test";

// Meetings: the forum policies, the meeting list, the meeting room with its agenda, participants
// and positions, the gated writes (a refusal stays the server's), and the states.

test.describe.configure({ mode: "serial" });

const MEETING = {
  id: "00000000-0000-4000-8000-000000000070",
  card_name: "S1 steering meeting",
  status: "scheduled",
  meeting_type_name: "Steering committee",
  meeting_type: "steering",
  meeting_type_version: 1,
  decision_rule: "chair decides",
};
const DETAIL = {
  meeting: MEETING,
  instruments: { open_items: 1 },
  agenda: [
    { id: "00000000-0000-4000-8000-000000000071", sequence: 1, resolved_kind: "decision", outcome: null },
  ],
  attendance: [
    {
      seat: "chair",
      participant_kind: "human",
      participant_id: "00000000-0000-4000-8000-000000000072",
      response_state: "accepted",
    },
  ],
  positions: [],
};

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

test("the meeting list and the forum policies render", async ({ page }) => {
  await page.route("**/api/actions/meetings.list/invocations", (route) =>
    json(route, { items: [MEETING] }),
  );
  await page.route("**/api/actions/meetings.type_list/invocations", (route) =>
    json(route, { items: [MEETING] }),
  );
  await page.goto("/meetings");
  await expect(page.getByRole("heading", { name: "Meetings", level: 1 })).toBeVisible();
  await expect(page.getByTestId("meetings-list")).toContainText("S1 steering meeting");
  await expect(page.getByTestId("meetings-policies")).toContainText("Steering committee");
});

test("the room renders the agenda, participants and the gated open", async ({ page }) => {
  await page.route("**/api/actions/meetings.get/invocations", (route) => json(route, DETAIL));
  await page.goto(`/meetings/${MEETING.id}`);
  await expect(page.getByRole("heading", { name: "S1 steering meeting" })).toBeVisible();
  await expect(page.getByTestId("meetings-agenda")).toContainText("decision");
  await expect(page.getByTestId("meetings-participants")).toContainText("chair");
  await expect(page.getByRole("button", { name: "Open meeting" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Close item" })).toBeVisible();
});

test("an unseated write is the server's meeting_not_permitted refusal", async ({ page }) => {
  await page.route("**/api/actions/meetings.get/invocations", (route) => json(route, DETAIL));
  await page.route("**/api/actions/meetings.open/invocations", (route) =>
    route.fulfill({
      status: 403,
      contentType: "application/problem+json",
      body: JSON.stringify({
        type: "action-forbidden",
        title: "Forbidden",
        status: 403,
        code: "meeting_not_permitted",
        detail: "the principal holds no seat at this meeting",
        request_id: "req-7",
      }),
    }),
  );

  await page.goto(`/meetings/${MEETING.id}`);
  await page.getByRole("button", { name: "Open meeting" }).click();
  const dialog = page.getByRole("dialog", { name: "Open meeting" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect(dialog.locator(".form-error")).toContainText("meeting_not_permitted");
  await dialog.getByRole("button", { name: "Close" }).click();
});

test("the empty lists are the app's own messages", async ({ page }) => {
  await page.route("**/api/actions/meetings.list/invocations", (route) => json(route, { items: [] }));
  await page.route("**/api/actions/meetings.type_list/invocations", (route) =>
    json(route, { items: [] }),
  );
  await page.goto("/meetings");
  await expect(
    page.getByText("No meeting in this tenant yet. One is scheduled from its type through meetings.schedule.", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("No forum policy in this tenant yet. A meeting schedules its first version.", { exact: true }),
  ).toBeVisible();
});

test("the loading state is shown while the list read is in flight", async ({ page }) => {
  await page.route("**/api/actions/meetings.list/invocations", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/meetings");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/meetings.list/invocations", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the meetings read failed",
      }),
    }),
  );
  await page.goto("/meetings");
  await expect(page.locator(".problem")).toContainText("the meetings read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/meetings");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/meetings");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
