import { useState, useEffect } from "react";
import { getRuntimeDetectionHint, configureRuntime } from "../../api/runtimeApi";
import { resumeGoalRun } from "../../api/goalRunApi";
import { useAppStore } from "../../store/useAppStore";
import type { ProjectRuntimeSpec, RuntimeReadinessCheck } from "../../types";

// Build a minimal RuntimeReadinessCheck from kind + port
function buildReadinessCheck(kind: "none" | "http" | "tcp", _port: string): RuntimeReadinessCheck {
  if (kind === "http") {
    return { kind: "http", path: "/", expectedStatus: 200, timeoutSeconds: 90, pollIntervalMs: 500 };
  }
  if (kind === "tcp") {
    return { kind: "tcpPort", timeoutSeconds: 90, pollIntervalMs: 500 };
  }
  return { kind: "none" };
}

interface Props {
  projectId: string;
  goalRunId: string;
  onApplied: () => void;
}

export function QuickRuntimeSetup({ projectId, goalRunId, onApplied }: Props) {
  const goToSettings = useAppStore((s) => s.goToSettings);
  const [hint, setHint] = useState<ProjectRuntimeSpec | null>(null);
  const [loadingHint, setLoadingHint] = useState(true);

  const [runCommand, setRunCommand] = useState("");
  const [installCommand, setInstallCommand] = useState("");
  const [port, setPort] = useState("");
  const [readinessKind, setReadinessKind] = useState<"none" | "http" | "tcp">("none");
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getRuntimeDetectionHint(projectId)
      .then((h) => {
        setHint(h);
        if (h) {
          setRunCommand(h.runCommand ?? "");
          setInstallCommand(h.installCommand ?? "");
          setPort(h.portHint != null ? String(h.portHint) : "");
          // Detect readiness kind from hint
          const rc = h.readinessCheck;
          if (rc && typeof rc === "object" && "kind" in rc) {
            if (rc.kind === "http") setReadinessKind("http");
            else if (rc.kind === "tcpPort") setReadinessKind("tcp");
            else setReadinessKind("none");
          }
        }
      })
      .catch(() => {
        // hint failed — form starts empty, operator fills manually
      })
      .finally(() => setLoadingHint(false));
  }, [projectId]);

  const handleApply = async () => {
    if (!runCommand.trim()) {
      setError("Run command is required");
      return;
    }
    setApplying(true);
    setError(null);
    try {
      const portNum = port ? parseInt(port, 10) : null;
      const spec: ProjectRuntimeSpec = {
        runCommand: runCommand.trim(),
        installCommand: installCommand.trim() || null,
        portHint: portNum,
        appUrl: portNum ? `http://127.0.0.1:${portNum}` : null,
        readinessCheck: buildReadinessCheck(readinessKind, port),
        verifyCommand: null,
        stopBehavior: { kind: "kill" },
      };
      await configureRuntime(projectId, spec);
      await resumeGoalRun(goalRunId);
      onApplied();
    } catch (e) {
      setError(String(e));
    } finally {
      setApplying(false);
    }
  };

  if (loadingHint) {
    return (
      <div className="rounded border border-blue-900/40 bg-blue-950/20 p-3 text-[11px]">
        <p className="text-blue-300 animate-pulse">Detecting runtime configuration…</p>
      </div>
    );
  }

  return (
    <div className="rounded border border-blue-900/40 bg-blue-950/20 p-3 text-[11px]">
      <p className="font-medium text-blue-300 mb-1">Quick Runtime Setup</p>
      <p className="text-gray-400 mb-3">
        {hint
          ? "We detected a partial configuration. Confirm or adjust and click Apply & Retry."
          : "Runtime detection couldn't determine how to run this project. Fill in the fields below."}
      </p>
      <div className="space-y-2">
        <div>
          <label className="block text-gray-400 mb-0.5">Run command <span className="text-red-400">*</span></label>
          <input
            value={runCommand}
            onChange={(e) => setRunCommand(e.target.value)}
            placeholder="e.g. npm run dev"
            className="w-full rounded border border-gray-700 bg-gray-900 px-2 py-1 text-gray-200 focus:border-blue-500 focus:outline-none"
          />
        </div>
        <div>
          <label className="block text-gray-400 mb-0.5">Install command <span className="text-gray-600">(optional)</span></label>
          <input
            value={installCommand}
            onChange={(e) => setInstallCommand(e.target.value)}
            placeholder="e.g. npm install"
            className="w-full rounded border border-gray-700 bg-gray-900 px-2 py-1 text-gray-200 focus:border-blue-500 focus:outline-none"
          />
        </div>
        <div>
          <label className="block text-gray-400 mb-0.5">Port <span className="text-gray-600">(optional, for web apps)</span></label>
          <input
            value={port}
            onChange={(e) => setPort(e.target.value)}
            placeholder="e.g. 3000"
            type="number"
            className="w-full rounded border border-gray-700 bg-gray-900 px-2 py-1 text-gray-200 focus:border-blue-500 focus:outline-none"
          />
        </div>
        <div>
          <label className="block text-gray-400 mb-1">Readiness check</label>
          <div className="flex gap-2">
            {(["none", "http", "tcp"] as const).map((k) => (
              <button
                key={k}
                onClick={() => setReadinessKind(k)}
                className={`rounded px-2 py-0.5 text-[10px] font-medium transition-colors ${
                  readinessKind === k
                    ? "bg-blue-700 text-white"
                    : "border border-gray-700 text-gray-400 hover:text-gray-200"
                }`}
              >
                {k === "none" ? "None" : k === "http" ? "HTTP" : "TCP Port"}
              </button>
            ))}
          </div>
        </div>
      </div>
      {error && <p className="mt-2 text-red-400">{error}</p>}
      <div className="mt-3 flex items-center gap-2">
        <button
          onClick={() => void handleApply()}
          disabled={applying || !runCommand.trim()}
          className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-500 disabled:opacity-50 transition-colors"
        >
          {applying ? "Applying…" : "Apply & Retry"}
        </button>
        <button
          onClick={goToSettings}
          className="text-[10px] text-gray-500 hover:text-gray-300 transition-colors"
        >
          Open Settings →
        </button>
      </div>
    </div>
  );
}
