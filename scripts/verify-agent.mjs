#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const feature = process.env.FEATURE?.trim();

if (!feature) {
  console.error("ERROR: FEATURE is required, for example: FEATURE=forced-fail-repair npm run verify:agent");
  process.exit(1);
}

const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
const sessionRoot = path.join(process.cwd(), ".debug-sessions", timestamp);
const evidenceRoot = path.join(sessionRoot, "verification");
const featureEvidenceDir = path.join(evidenceRoot, feature);
fs.mkdirSync(featureEvidenceDir, { recursive: true });

const env = {
  ...process.env,
  PROJECT_BUILDER_EVIDENCE_DIR: evidenceRoot,
};

const result = spawnSync(
  process.platform === "win32" ? "npx.cmd" : "npx",
  ["playwright", "test", "--grep", `@feature-${feature}`],
  {
    cwd: process.cwd(),
    env,
    encoding: "utf8",
  },
);

const summary = {
  feature,
  startedAt: timestamp,
  finishedAt: new Date().toISOString(),
  status: result.status === 0 ? "passed" : "failed",
  exitCode: result.status,
  signal: result.signal,
  evidenceDir: featureEvidenceDir,
};

fs.writeFileSync(
  path.join(featureEvidenceDir, "command-output.txt"),
  [result.stdout, result.stderr].filter(Boolean).join("\n"),
);
fs.writeFileSync(
  path.join(featureEvidenceDir, "summary.json"),
  JSON.stringify(summary, null, 2),
);

if (result.stdout) process.stdout.write(result.stdout);
if (result.stderr) process.stderr.write(result.stderr);
console.log(`Evidence bundle: ${featureEvidenceDir}`);

process.exit(result.status ?? 1);
