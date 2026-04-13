import type {
  Project,
  ProjectRuntimeSpec,
  ProjectRuntimeStatus,
  RuntimeLogTail,
} from "../types";
import { loggedInvoke } from "./runtime";

export async function configureRuntime(
  projectId: string,
  spec: ProjectRuntimeSpec,
): Promise<Project> {
  return loggedInvoke("configure_runtime", { projectId, spec });
}

export async function getRuntimeStatus(
  projectId: string,
): Promise<ProjectRuntimeStatus> {
  return loggedInvoke("get_runtime_status", { projectId });
}

export async function detectRuntime(
  projectId: string,
): Promise<ProjectRuntimeSpec | null> {
  return loggedInvoke("detect_runtime", { projectId });
}

export async function detectRuntimeWithAgent(
  projectId: string,
): Promise<ProjectRuntimeSpec | null> {
  return loggedInvoke("detect_runtime_with_agent", { projectId });
}

export async function startRuntime(
  projectId: string,
): Promise<ProjectRuntimeStatus> {
  return loggedInvoke("start_runtime", { projectId });
}

export async function stopRuntime(
  projectId: string,
): Promise<ProjectRuntimeStatus> {
  return loggedInvoke("stop_runtime", { projectId });
}

export async function tailRuntimeLogs(
  projectId: string,
  limit?: number,
): Promise<RuntimeLogTail> {
  return loggedInvoke("tail_runtime_logs", { projectId, limit: limit ?? null });
}

export async function verifyRuntime(projectId: string): Promise<string> {
  return loggedInvoke("verify_runtime", { projectId });
}
