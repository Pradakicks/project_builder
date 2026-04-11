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
  phaseControl: PhaseControlPolicy;
  conflictResolution: ConflictResolutionPolicy;
  workingDirectory: string | null;
  defaultExecutionEngine: string | null;
  postRunValidationCommand: string | null;
}

export interface LlmConfig {
  provider: string;
  model: string;
  apiKeyEnv: string | null;
  baseUrl: string | null;
}

export type PhaseControlPolicy = "manual" | "gated-auto-advance" | "fully-autonomous";
export type ConflictResolutionPolicy = "manual" | "ai-assisted" | "auto-resolve";

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
  actionsJson: string;
  createdAt: string;
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
