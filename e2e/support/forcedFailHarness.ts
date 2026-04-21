import type { Page } from "@playwright/test";

const now = "2026-04-20T12:00:00.000Z";
const projectId = "e2e-project-forced-fail";
const pieceId = "e2e-piece-server";
const planId = "e2e-plan-forced-fail";
const goalRunId = "e2e-goal-run-forced-fail";
const runtimeLogPath = "/tmp/project-builder-e2e/runtime.log";

export async function installForcedFailHarness(page: Page) {
  await page.addInitScript(
    ({ now, projectId, pieceId, planId, goalRunId, runtimeLogPath }) => {
      const clone = (value: unknown) => JSON.parse(JSON.stringify(value));
      const project = {
        id: projectId,
        name: "Autopilot Verification Fixture",
        description: "Seeded forced-fatal repair loop for browser verification.",
        rootPieceId: pieceId,
        settings: {
          llmConfigs: [],
          defaultTokenBudget: 0,
          autonomyMode: "autopilot",
          phaseControl: "fully-autonomous",
          conflictResolution: "manual",
          workingDirectory: null,
          defaultExecutionEngine: null,
          postRunValidationCommand: null,
          runtimeSpec: null,
        },
        createdAt: now,
        updatedAt: now,
      };
      const piece = {
        id: pieceId,
        projectId,
        parentId: null,
        name: "Minimal Node.js HTTP Server",
        pieceType: "service",
        color: "#22d3ee",
        icon: null,
        responsibilities: "Serve a tiny verification endpoint.",
        interfaces: [],
        constraints: [],
        notes: "",
        agentPrompt: "",
        agentConfig: {
          provider: null,
          model: null,
          tokenBudget: null,
          activeAgents: [],
          executionEngine: null,
          timeout: null,
        },
        outputMode: "both",
        phase: "approved",
        positionX: 0,
        positionY: 0,
        createdAt: now,
        updatedAt: now,
      };
      const task = {
        id: "e2e-task-server",
        pieceId,
        pieceName: piece.name,
        title: "Implement Node.js HTTP Server",
        description: "Serve the forced-fatal smoke endpoint.",
        priority: "high",
        suggestedPhase: "implementation",
        dependencies: [],
        status: "complete",
        order: 1,
      };
      const plan = {
        id: planId,
        projectId,
        version: 1,
        status: "approved",
        summary: "Build and verify the forced-fatal smoke fixture.",
        userGuidance: "",
        tasks: [task],
        rawOutput: "",
        tokensUsed: 0,
        integrationReview: "",
        createdAt: now,
        updatedAt: now,
      };
      const runtimeStatus = {
        projectId,
        spec: {
          installCommand: "npm install",
          runCommand: "npm start",
          readinessCheck: { kind: "none" },
          verifyCommand: null,
          stopBehavior: { kind: "kill" },
          appUrl: "http://127.0.0.1:3030",
          acceptanceSuite: {
            checks: [
              {
                kind: "logScan",
                name: "log scan - fatal patterns",
                patterns: ["(?i)FATAL"],
                mode: "mustNotMatch",
                lastNLines: 200,
              },
              {
                kind: "httpProbe",
                name: "http probe - root",
                path: "/",
                expectedStatusMin: 200,
                expectedStatusMax: 299,
                expectedBodyContains: "\"service\":\"verify-smoke\"",
              },
            ],
            stopOnFirstFailure: false,
          },
          portHint: 3030,
        },
        session: {
          sessionId: "e2e-runtime-session",
          status: "running",
          startedAt: now,
          updatedAt: now,
          endedAt: null,
          url: "http://127.0.0.1:3030",
          portHint: 3030,
          logPath: runtimeLogPath,
          recentLogs: [
            "[runtime] snapshot line from earlier run",
            "[stdout] older warning retained for history",
          ],
          lastError: null,
          exitCode: null,
          pid: 3030,
        },
      };
      const liveLogs = [
        "[runtime] running install command",
        "[runtime] install command: npm install",
        "[stdout] up to date, audited 66 packages in 326ms",
        "[runtime] spawning run command",
        "[stdout] > verify-smoke@1.0.0 start",
        "[stdout] > node server.js",
        "[stdout] FATAL: forced for test",
        "[stdout] verify-smoke listening on http://127.0.0.1:3030",
      ];
      const verificationResult = {
        passed: false,
        startedAt: now,
        finishedAt: now,
        message: "log scan - fatal patterns: matched forbidden /(?i)FATAL/ on line 7",
        checks: [
          {
            kind: "logScan",
            name: "log scan - fatal patterns",
            passed: false,
            detail: "matched forbidden /(?i)FATAL/ on line 7",
            durationMs: 3,
            expected: "No lines matching /(?i)FATAL/",
            actual: "[stdout] FATAL: forced for test",
          },
          {
            kind: "http",
            name: "http probe - root",
            passed: true,
            detail: "200 OK",
            durationMs: 1,
            expected: "status 200-299",
            actual: "{\"ok\":false,\"service\":\"verify-smoke\"}",
          },
        ],
      };
      const makeRun = (overrides = {}) => ({
        id: goalRunId,
        projectId,
        prompt:
          "Generate a work plan for the existing piece and run it end-to-end. Then configure the runtime and start it.",
        phase: "verification",
        status: "blocked",
        blockerReason: verificationResult.message,
        currentPlanId: planId,
        runtimeStatusSummary: "npm start",
        verificationSummary: JSON.stringify(verificationResult),
        retryCount: 3,
        lastFailureSummary: "Earlier repair attempt: runtime exited before verification completed",
        stopRequested: false,
        currentPieceId: pieceId,
        currentTaskId: task.id,
        retryBackoffUntil: null,
        lastFailureFingerprint: "verification-log-scan-fatal",
        attentionRequired: true,
        lastHeartbeatAt: null,
        operatorRepairRequested: false,
        createdAt: now,
        updatedAt: now,
        ...overrides,
      });
      const state = {
        project,
        piece,
        plan,
        task,
        runtimeStatus,
        liveLogs,
        run: makeRun(),
        events: [
          {
            id: "event-blocked",
            goalRunId,
            phase: "verification",
            kind: "blocked",
            summary: verificationResult.message,
            payloadJson: JSON.stringify({ fingerprint: "verification-log-scan-fatal" }),
            createdAt: now,
          },
          {
            id: "event-retry",
            goalRunId,
            phase: "verification",
            kind: "retry-scheduled",
            summary: `Repair attempt 3/3: ${verificationResult.message}`,
            payloadJson: JSON.stringify({
              fingerprint: "verification-log-scan-fatal",
              retryCount: 3,
            }),
            createdAt: "2026-04-20T11:59:30.000Z",
          },
        ],
        invocations: [],
      };
      const deliverySnapshot = () => ({
        goalRun: state.run,
        currentPlan: state.plan,
        blockingPiece: state.piece,
        blockingTask: state.task,
        retryState: {
          retryCount: state.run.retryCount,
          stopRequested: state.run.stopRequested,
          retryBackoffUntil: state.run.retryBackoffUntil,
          lastFailureSummary: state.run.lastFailureSummary,
          lastFailureFingerprint: state.run.lastFailureFingerprint,
          attentionRequired: state.run.attentionRequired,
          operatorRepairRequested: state.run.operatorRepairRequested,
        },
        codeEvidence: {
          pieceId,
          pieceName: state.piece.name,
          gitBranch: "piece/minimal-node-js-http-server",
          gitCommitSha: "e2e1234",
          gitDiffStat: "server.js | 12 ++++++++++++",
          generatedFilesArtifact: {
            id: "artifact-generated-files",
            pieceId,
            agentId: null,
            artifactType: "generated-files",
            title: "Generated files",
            content: "server.js\npackage.json\nruntime.json",
            reviewStatus: "approved",
            version: 1,
            createdAt: now,
            updatedAt: now,
          },
        },
        runtimeStatus: state.runtimeStatus,
        recentEvents: [...state.events].reverse(),
        liveActivity: null,
        verificationResult,
      });
      window.__PROJECT_BUILDER_E2E__ = {
        async invoke(cmd, args) {
          state.invocations.push({ cmd, args: args ?? null, at: new Date().toISOString() });
          switch (cmd) {
            case "list_projects":
              return clone([state.project]);
            case "get_project":
              return clone(state.project);
            case "list_pieces":
              return clone([state.piece]);
            case "list_children":
            case "list_connections":
            case "list_interrupted_runs":
            case "get_agent_history":
            case "list_artifacts":
            case "list_cto_decisions":
            case "list_team_briefs":
            case "list_teams_for_project":
              return [];
            case "list_work_plans":
              return clone([state.plan]);
            case "get_work_plan":
              return clone(state.plan);
            case "list_goal_runs":
              return clone([state.run]);
            case "get_goal_run":
              return clone(state.run);
            case "get_goal_run_delivery_snapshot":
              return clone(deliverySnapshot());
            case "get_runtime_status":
              return clone(state.runtimeStatus);
            case "tail_runtime_logs":
              return clone({ path: runtimeLogPath, lines: state.liveLogs });
            case "resume_goal_run_with_repair": {
              const requestedAt = new Date().toISOString();
              state.events.push({
                id: `event-repair-requested-${state.events.length}`,
                goalRunId,
                phase: "verification",
                kind: "repair-requested",
                summary: "Repair requested by operator",
                payloadJson: null,
                createdAt: requestedAt,
              });
              state.run = makeRun({
                status: "blocked",
                operatorRepairRequested: false,
                updatedAt: requestedAt,
              });
              return clone(makeRun({
                status: "running",
                blockerReason: null,
                lastFailureSummary: null,
                lastFailureFingerprint: null,
                attentionRequired: false,
                operatorRepairRequested: true,
                updatedAt: requestedAt,
              }));
            }
            case "rerun_verification":
            case "resume_goal_run":
            case "stop_goal_run":
            case "pause_goal_run":
            case "cancel_goal_run":
              return clone(state.run);
            case "start_runtime":
            case "stop_runtime":
              return clone(state.runtimeStatus);
            default:
              return null;
          }
        },
        async listen() {
          return () => {};
        },
        snapshot() {
          return clone({
            run: state.run,
            events: state.events,
            runtimeStatus: state.runtimeStatus,
            liveLogs: state.liveLogs,
            invocations: state.invocations,
            deliverySnapshot: deliverySnapshot(),
          });
        },
      };
    },
    { now, projectId, pieceId, planId, goalRunId, runtimeLogPath },
  );
}
