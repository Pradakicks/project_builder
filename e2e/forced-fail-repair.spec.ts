import { expect, test } from "@playwright/test";
import { collectEvidence } from "./support/evidence";
import { installForcedFailHarness } from "./support/forcedFailHarness";

const featureId = "forced-fail-repair";

test("forced fatal repair loop produces fresh operator evidence @feature-forced-fail-repair", async ({
  page,
}, testInfo) => {
  page.on("console", (message) => {
    if (message.type() === "error") {
      console.log(`browser console error: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => {
    console.log(`browser page error: ${error.stack ?? error.message}`);
  });
  await installForcedFailHarness(page);

  await page.goto("/");
  await page.getByRole("button", { name: /Autopilot Verification Fixture/ }).click();
  await page.getByTitle("Open Delivery").click();

  await expect(page.getByTestId("delivery-panel")).toBeVisible();
  await expect(page.getByTestId("delivery-current-blocker")).toContainText("FATAL");
  await expect(page.getByTestId("delivery-previous-failures")).toContainText("Historical");
  await expect(page.getByTestId("delivery-runtime-log-source")).toContainText("live");

  await page.getByTestId("delivery-resume-with-repair-button").click();

  await expect(page.getByTestId("delivery-action-receipt")).toContainText("Repair requested");
  await expect(page.getByTestId("delivery-repair-status")).toContainText("Repair requested by operator");

  const snapshot = await page.evaluate(() => window.__PROJECT_BUILDER_E2E__?.snapshot?.()) as {
    events: Array<{ kind: string }>;
    invocations: Array<{ cmd: string }>;
  } | undefined;
  expect(snapshot?.events.at(-1)?.kind).toBe("repair-requested");
  expect(snapshot?.invocations.some((entry) => entry.cmd === "resume_goal_run_with_repair")).toBe(true);

  await collectEvidence(page, testInfo, featureId, snapshot);
});
