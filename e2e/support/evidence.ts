import fs from "node:fs";
import path from "node:path";
import type { Page, TestInfo } from "@playwright/test";

function sanitizeFeatureId(value: string) {
  return value.replace(/[^a-zA-Z0-9._-]/g, "-");
}

export function evidenceDir(testInfo: TestInfo, featureId: string) {
  const explicit = process.env.PROJECT_BUILDER_EVIDENCE_DIR;
  const dir = explicit
    ? path.join(explicit, sanitizeFeatureId(featureId))
    : path.join(testInfo.outputDir, "evidence", sanitizeFeatureId(featureId));
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

export async function collectEvidence(
  page: Page,
  testInfo: TestInfo,
  featureId: string,
  payload: unknown,
) {
  const dir = evidenceDir(testInfo, featureId);
  const statePath = path.join(dir, "debug-report.json");
  const screenshotPath = path.join(dir, "delivery.png");

  fs.writeFileSync(
    statePath,
    JSON.stringify(
      {
        featureId,
        generatedAt: new Date().toISOString(),
        status: testInfo.status,
        expectedStatus: testInfo.expectedStatus,
        payload,
      },
      null,
      2,
    ),
  );
  await page.screenshot({ path: screenshotPath, fullPage: true });

  await testInfo.attach("debug-report", {
    path: statePath,
    contentType: "application/json",
  });
  await testInfo.attach("delivery-screenshot", {
    path: screenshotPath,
    contentType: "image/png",
  });

  return { dir, statePath, screenshotPath };
}
