import { expect, test } from "./fixture";

import { invoke } from "./helpers";

// T18-3 P1: the T16 intake and both publication gates. The screen reads the tenant's rows and
// writes only through the owning actions; a converted draft is not publishable until its source
// has been reviewed by a person (the server says so, and the screen repeats it verbatim).
//
// The fixture is the T16 BPMN document (`crates/actions/tests/fixtures/t16/process.bpmn`): the
// conversion is deterministic (no model call), so this spec does not depend on model output.
const BPMN = `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL"
                  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                  id="definitions-t18-fixture"
                  targetNamespace="http://example.invalid/t18">
  <bpmn:process id="process_receiving" name="Synthetic receiving process" isExecutable="false">
    <bpmn:startEvent id="start_receiving" name="Delivery arrives">
      <bpmn:outgoing>flow_1</bpmn:outgoing>
    </bpmn:startEvent>
    <bpmn:task id="task_verify" name="Verify the delivery">
      <bpmn:incoming>flow_1</bpmn:incoming>
      <bpmn:outgoing>flow_2</bpmn:outgoing>
    </bpmn:task>
    <bpmn:endEvent id="end_receiving" name="Delivery stored">
      <bpmn:incoming>flow_2</bpmn:incoming>
    </bpmn:endEvent>
    <bpmn:sequenceFlow id="flow_1" sourceRef="start_receiving" targetRef="task_verify"/>
    <bpmn:sequenceFlow id="flow_2" sourceRef="task_verify" targetRef="end_receiving"/>
  </bpmn:process>
</bpmn:definitions>`;

test("an unreviewed source cannot publish", async ({ page, request }) => {
  const stamp = Date.now();
  const key = `e2e-proc-${stamp}`;
  const ingest = await invoke<{ procedure_id: string; process_id: string }>(
    request,
    "procedures.ingest_process",
    {
      title: `E2E unreviewed source ${stamp}`,
      process_key: key,
      source_key: `source-${stamp}`,
      source: { kind: "bpmn", media_type: "application/xml", text: BPMN },
    },
  );
  expect(ingest.procedure_id).toBeTruthy();
  expect(ingest.process_id).toBeTruthy();

  await page.goto("/procedures");
  await expect(page.getByRole("heading", { name: "Procedures" })).toBeVisible();

  // The workspace is the tenant's own read: the ingested judge is a draft row.
  const judge = page.getByTestId("procedures-table").locator("tr", { hasText: key });
  await expect(judge).toBeVisible();
  await expect(judge.getByText("draft", { exact: true })).toBeVisible();

  await judge.getByRole("button", { name: "Select" }).click();
  await expect(page.getByRole("button", { name: "Publish the judge" })).toBeVisible();
  await page.getByRole("button", { name: "Publish the judge" }).click();

  // The server's refusal names the unreviewed source; the screen shows it, not a success.
  const refusal = page.locator("p.notice[role='alert']");
  await expect(refusal).toContainText("Publish refused");
  await expect(refusal).toContainText(/human must review/i);

  // The judge stayed draft, and the read never turned into an error panel. The tab bar is a
  // tablist, so the tab's role is `tab`, not `button`.
  await page.getByRole("tab", { name: "Workspace" }).click();
  await expect(
    page.getByTestId("procedures-table").locator("tr", { hasText: key }).getByText("draft", { exact: true }),
  ).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a PDDL step the parser rejects refuses and names the step", async ({ page, request }) => {
  const stamp = Date.now();
  const key = `e2e-pddl-${stamp}`;
  const created = await invoke<{ id: string; lock_version: number }>(request, "processes.create", {
    key,
    title: `E2E bad PDDL ${stamp}`,
  });
  await invoke(
    request,
    "processes.update",
    {
      process_id: created.id,
      steps: [
        {
          kind: "computed",
          actor: "system",
          action: "cards.comment",
          input_template: {},
          action_text: "this is not a PDDL action",
        },
      ],
    },
    created.lock_version,
  );

  await page.goto("/procedures");
  const process = page.getByTestId("procedures-processes").locator("tr", { hasText: key });
  await expect(process).toBeVisible();
  await process.getByRole("button", { name: "Select" }).click();

  await expect(page.getByTestId("procedures-steps")).toBeVisible();
  await page.getByRole("button", { name: "Validate the draft" }).click();
  const refusal = page.locator("div.callout.bad[role='alert']");
  await expect(refusal).toContainText("Validation refused");
  await expect(refusal).toContainText(/step 1/i);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the workspace and the capability catalogue read the tenant's rows", async ({
  page,
  request,
}) => {
  const stamp = Date.now();
  const key = `e2e-workspace-${stamp}`;
  await invoke(request, "procedures.create", {
    key,
    title: `E2E workspace ${stamp}`,
  });

  await page.goto("/procedures");
  await expect(page.getByTestId("procedures-table").locator("tr", { hasText: key })).toBeVisible();
  await expect(page.getByText("Capabilities", { exact: true })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});
