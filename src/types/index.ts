// Mirrors Rust data models exactly

export interface Project {
  id: string;
  name: string;
  description: string;
  rootPieceId: string | null;
  settings: ProjectSettings;
  createdAt: string;
  updatedAt: string;
}

export interface ProjectSettings {
  llmConfigs: LlmConfig[];
  defaultTokenBudget: number;
  autonomyMode: AutonomyMode;
  phaseControl: PhaseControlPolicy;
  conflictResolution: ConflictResolutionPolicy;
  workingDirectory: string | null;
  defaultExecutionEngine: string | null;
  postRunValidationCommand: string | null;
  runtimeSpec: ProjectRuntimeSpec | null;
}

export interface LlmConfig {
  provider: string;
  model: string;
  apiKeyEnv: string | null;
  baseUrl: string | null;
}

export type PhaseControlPolicy = "manual" | "gated-auto-advance" | "fully-autonomous";
export type ConflictResolutionPolicy = "manual" | "ai-assisted" | "auto-resolve";
export type AutonomyMode = "manual" | "guided" | "autopilot";

export interface Piece {
  id: string;
  projectId: string;
  parentId: string | null;
  name: string;
  pieceType: string;
  color: string | null;
  icon: string | null;
  responsibilities: string;
  interfaces: PieceInterface[];
  constraints: Constraint[];
  notes: string;
  agentPrompt: string;
  agentConfig: AgentConfig;
  outputMode: OutputMode;
  phase: Phase;
  positionX: number;
  positionY: number;
  createdAt: string;
  updatedAt: string;
}

export interface PieceInterface {
  name: string;
  direction: InterfaceDirection;
  description: string;
}

export type InterfaceDirection = "in" | "out";

export interface Constraint {
  category: string;
  description: string;
}

export interface AgentConfig {
  provider: string | null;
  model: string | null;
  tokenBudget: number | null;
  activeAgents: string[];
  executionEngine: string | null;
  timeout: number | null;
}

export type OutputMode = "docs-only" | "code-only" | "both";
export type Phase = "design" | "review" | "approved" | "implementing";

export interface Connection {
  id: string;
  projectId: string;
  sourcePieceId: string;
  targetPieceId: string;
  direction: Direction;
  label: string;
  dataType: string | null;
  protocol: string | null;
  constraints: Constraint[];
  notes: string;
  metadata: Record<string, string>;
}

export type Direction = "unidirectional" | "bidirectional";

export type AgentRole = "leader" | "implementation" | "testing" | "review" | "custom";
export type AgentState = "idle" | "working" | "waiting-for-approval" | "blocked" | "error";
export type LlmProvider = "claude" | "openai" | "local" | "custom";
export type ReviewStatus = "draft" | "in-review" | "approved" | "rejected";

export interface PieceUpdate {
  name?: string;
  pieceType?: string;
  color?: string;
  icon?: string;
  responsibilities?: string;
  interfaces?: PieceInterface[];
  constraints?: Constraint[];
  notes?: string;
  agentPrompt?: string;
  agentConfig?: AgentConfig;
  outputMode?: OutputMode;
  phase?: Phase;
  positionX?: number;
  positionY?: number;
}

export interface ConnectionUpdate {
  label?: string;
  direction?: Direction;
  dataType?: string;
  protocol?: string;
  constraints?: Constraint[];
  notes?: string;
  metadata?: Record<string, string>;
}

// ── Artifacts ────────────────────────────────────────────

export interface Artifact {
  id: string;
  pieceId: string;
  agentId: string | null;
  artifactType: string;
  title: string;
  content: string;
  reviewStatus: ReviewStatus;
  version: number;
  createdAt: string;
  updatedAt: string;
}

export interface TokenUsage {
  input: number;
  output: number;
}

export interface ValidationResult {
  command: string;
  passed: boolean;
  exitCode: number;
  output: string;
}

export type RuntimeSessionStatus = "idle" | "starting" | "running" | "stopping" | "stopped" | "failed";

export type RuntimeReadinessCheck =
  | { kind: "none" }
  | {
      kind: "http";
      path?: string;
      expectedStatus?: number;
      timeoutSeconds?: number;
      pollIntervalMs?: number;
    }
  | {
      kind: "tcpPort";
      timeoutSeconds?: number;
      pollIntervalMs?: number;
    };

export type RuntimeStopBehavior =
  | { kind: "kill" }
  | {
      kind: "graceful";
      timeoutSeconds?: number;
    };

export interface ProjectRuntimeSpec {
  installCommand: string | null;
  runCommand: string;
  readinessCheck: RuntimeReadinessCheck;
  verifyCommand: string | null;
  stopBehavior: RuntimeStopBehavior;
  appUrl: string | null;
  portHint: number | null;
}

export interface ProjectRuntimeSession {
  sessionId: string;
  status: RuntimeSessionStatus;
  startedAt: string | null;
  updatedAt: string;
  endedAt: string | null;
  url: string | null;
  portHint: number | null;
  logPath: string | null;
  recentLogs: string[];
  lastError: string | null;
  exitCode: number | null;
  pid: number | null;
}

export interface ProjectRuntimeStatus {
  projectId: string;
  spec: ProjectRuntimeSpec | null;
  session: ProjectRuntimeSession | null;
}

export interface RuntimeLogTail {
  path: string | null;
  lines: string[];
}

// ── Goal Runs ───────────────────────────────────────────

export type GoalRunPhase =
  | "prompt-received"
  | "planning"
  | "implementation"
  | "merging"
  | "runtime-configuration"
  | "runtime-execution"
  | "verification";

export type GoalRunStatus = "running" | "retrying" | "blocked" | "completed" | "failed" | "interrupted";
export type GoalRunEventKind =
  | "phase-started"
  | "phase-completed"
  | "retry-scheduled"
  | "retry-resumed"
  | "blocked"
  | "failed"
  | "stopped"
  | "note";

export interface GoalRun {
  id: string;
  projectId: string;
  prompt: string;
  phase: GoalRunPhase;
  status: GoalRunStatus;
  blockerReason: string | null;
  currentPlanId: string | null;
  runtimeStatusSummary: string | null;
  verificationSummary: string | null;
  retryCount: number;
  lastFailureSummary: string | null;
  stopRequested: boolean;
  currentPieceId: string | null;
  currentTaskId: string | null;
  retryBackoffUntil: string | null;
  lastFailureFingerprint: string | null;
  attentionRequired: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface GoalRunUpdate {
  prompt?: string;
  phase?: GoalRunPhase;
  status?: GoalRunStatus;
  blockerReason?: string | null;
  currentPlanId?: string | null;
  runtimeStatusSummary?: string | null;
  verificationSummary?: string | null;
  retryCount?: number;
  lastFailureSummary?: string | null;
  stopRequested?: boolean;
  currentPieceId?: string | null;
  currentTaskId?: string | null;
  retryBackoffUntil?: string | null;
  lastFailureFingerprint?: string | null;
  attentionRequired?: boolean;
}

export interface GoalRunEvent {
  id: string;
  goalRunId: string;
  phase: GoalRunPhase;
  kind: GoalRunEventKind;
  summary: string;
  payloadJson: string | null;
  createdAt: string;
}

export interface GoalRunRetryState {
  retryCount: number;
  stopRequested: boolean;
  retryBackoffUntil: string | null;
  lastFailureSummary: string | null;
  lastFailureFingerprint: string | null;
  attentionRequired: boolean;
}

export interface GoalRunCodeEvidence {
  pieceId: string;
  pieceName: string;
  gitBranch: string | null;
  gitCommitSha: string | null;
  gitDiffStat: string | null;
  generatedFilesArtifact: Artifact | null;
}

export interface LiveActivity {
  pieceId: string;
  pieceName: string;
  taskId: string | null;
  taskTitle: string | null;
  engine: string | null;
  currentIndex: number;
  total: number;
}

export interface GoalRunDeliverySnapshot {
  goalRun: GoalRun;
  currentPlan: WorkPlan | null;
  blockingPiece: Piece | null;
  blockingTask: PlanTask | null;
  retryState: GoalRunRetryState;
  codeEvidence: GoalRunCodeEvidence | null;
  runtimeStatus: ProjectRuntimeStatus | null;
  recentEvents: GoalRunEvent[];
  liveActivity: LiveActivity | null;
}

export type GoalRunTimelineEntryKind =
  | "phase"
  | "runtime"
  | "verification"
  | "summary"
  | "history";

export interface GoalRunTimelineEntry {
  id: string;
  kind: GoalRunTimelineEntryKind;
  title: string;
  detail: string | null;
  timestamp: string;
  active: boolean;
  status: GoalRunStatus | "info" | "success" | "warning";
}

export interface AgentHistoryMetadata {
  usage?: TokenUsage | null;
  success?: boolean | null;
  exitCode?: number | null;
  phaseProposal?: string | null;
  phaseChanged?: string | null;
  gitBranch?: string | null;
  gitCommitSha?: string | null;
  gitDiffStat?: string | null;
  validation?: ValidationResult | null;
}

// ── CTO Decisions ───────────────────────────────────────

export interface CtoDecision {
  id: string;
  projectId: string;
  summary: string;
  review: CtoDecisionReview;
  execution: CtoDecisionExecution | null;
  rollback: CtoRollbackResult | null;
  status: CtoDecisionStatus;
  createdAt: string;
  updatedAt: string;
}

export type CtoDecisionStatus = "rejected" | "executed" | "failed" | "rolled-back";

export interface CtoDecisionReview {
  assistantText: string;
  cleanedContent: string;
  actions: CtoAction[];
  validationErrors: string[];
}

export interface CtoDecisionExecution {
  executed: number;
  errors: string[];
  steps: CtoActionExecutionStep[];
  switchToTab?: string | null;
  reloadCurrentProject: boolean;
  rollback: CtoRollbackPlan;
}

export interface CtoRollbackPlan {
  supported: boolean;
  reason?: string | null;
  steps: CtoRollbackStep[];
}

export interface CtoRollbackStep {
  index: number;
  action: string;
  description: string;
  supported: boolean;
  reason?: string | null;
  kind?: CtoRollbackKind | null;
}

export type CtoRollbackKind =
  | { kind: "restorePiece"; piece: Piece }
  | { kind: "deletePiece"; pieceId: string }
  | { kind: "restoreConnection"; connection: Connection }
  | { kind: "deleteConnection"; connectionId: string }
  | { kind: "restorePlanStatus"; planId: string; status: PlanStatus };

export interface CtoRollbackResult {
  appliedAt: string;
  steps: CtoRollbackResultStep[];
  errors: string[];
}

export interface CtoRollbackResultStep {
  index: number;
  action: string;
  description: string;
  status: "applied" | "failed" | "skipped";
  error?: string | null;
}

export interface CtoDecisionRecordInput {
  summary: string;
  review: CtoDecisionReview;
  execution: CtoDecisionExecution | null;
  status: CtoDecisionStatus;
}

export type CtoActionName =
  | "updatePiece"
  | "createPiece"
  | "runPiece"
  | "createConnection"
  | "updateConnection"
  | "generatePlan"
  | "approvePlan"
  | "rejectPlan"
  | "runAllTasks"
  | "mergeBranches"
  | "configureRuntime"
  | "runProject"
  | "stopProject"
  | "retryGoalStep";

export type CtoActionExecutionMode = "manual-review" | "autonomous-repair";

export interface CtoAction {
  action: CtoActionName;
  [key: string]: unknown;
}

export interface CtoActionReview {
  actions: CtoAction[];
  cleanedContent: string;
  validationErrors: string[];
}

export interface CtoActionExecutionStep {
  index: number;
  action: string;
  description: string;
  status: "executed" | "failed";
  error?: string;
  rollback?: CtoRollbackStep | null;
}

export interface CtoActionExecutionResult {
  executed: number;
  errors: string[];
  steps: CtoActionExecutionStep[];
  switchToTab?: string | null;
  reloadCurrentProject: boolean;
  rollback: CtoRollbackPlan;
}

// ── Work Plans ───────────────────────────────────────────

export type PlanStatus = "generating" | "draft" | "approved" | "rejected" | "superseded";
export type TaskPriority = "critical" | "high" | "medium" | "low";
export type TaskStatus = "pending" | "in-progress" | "complete" | "skipped";

export interface WorkPlan {
  id: string;
  projectId: string;
  version: number;
  status: PlanStatus;
  summary: string;
  userGuidance: string;
  tasks: PlanTask[];
  rawOutput: string;
  tokensUsed: number;
  integrationReview: string;
  createdAt: string;
  updatedAt: string;
}

export interface PlanTask {
  id: string;
  pieceId: string;
  pieceName: string;
  title: string;
  description: string;
  priority: TaskPriority;
  suggestedPhase: string;
  dependencies: string[];
  status: TaskStatus;
  order: number;
}

// ── Branch Merging ──────────────────────────────────────

export interface MergeSummary {
  merged: string[];
  skipped: string[];
  conflict: ConflictInfo | null;
  combinedDiffStat: string;
}

export interface ConflictInfo {
  pieceId: string;
  pieceName: string;
  branch: string;
  conflictingFiles: string[];
  conflictDiff: string;
}

export interface MergeProgressEvent {
  planId: string;
  pieceName: string;
  branch: string;
  status: "merging" | "merged" | "conflict" | "conflict-resolving" | "conflict-resolved" | "failed" | "skipped";
  message: string;
  current: number;
  total: number;
}

export interface IntegrationReviewChunk {
  planId: string;
  chunk: string;
  done: boolean;
}

// ── Developer Diagnostics ───────────────────────────────

export type DebugEventKind =
  | "frontend-log"
  | "ipc-invoke"
  | "ipc-result"
  | "ipc-error"
  | "cto-request"
  | "cto-response"
  | "cto-review"
  | "cto-decision"
  | "scenario"
  | "session";

export type DebugEventLevel = "debug" | "info" | "warn" | "error" | "trace";

export interface DebugEvent {
  id: string;
  timestamp: string;
  kind: DebugEventKind;
  level: DebugEventLevel;
  category: string;
  message: string;
  data?: unknown;
}

export interface DebugSessionSummary {
  enabled: boolean;
  sessionId: string | null;
  sessionDir: string | null;
  startedAt: string | null;
  logPath: string | null;
}

export interface DebugLogTail {
  path: string | null;
  lines: string[];
}

export interface DebugConversationMessage {
  role: string;
  content: string;
}

export type CapturedScenarioStatus = "failed" | "rejected";

export interface CapturedScenario {
  id: string;
  kind: "cto-chat";
  status: CapturedScenarioStatus;
  projectId: string;
  projectName: string | null;
  prompt: string;
  conversation: DebugConversationMessage[];
  assistantText: string | null;
  cleanedContent: string | null;
  review: CtoActionReview | null;
  decision: CtoDecisionRecordInput | null;
  error: string | null;
  capturedAt: string;
  path?: string | null;
}

export interface DebugReport {
  generatedAt: string;
  session: DebugSessionSummary | null;
  activeProjectId: string | null;
  activeView: string;
  lastScenario: CapturedScenario | null;
  recentEvents: DebugEvent[];
}
