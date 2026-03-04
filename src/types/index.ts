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
}

export interface LlmConfig {
  provider: string;
  model: string;
  apiKeyEnv: string | null;
  baseUrl: string | null;
}

export type PhaseControlPolicy = "manual" | "gated-auto-advance" | "fully-autonomous";

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
