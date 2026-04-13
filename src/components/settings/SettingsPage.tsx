import { useState, useEffect } from "react";
import * as api from "../../api/projectApi";
import * as runtimeApi from "../../api/runtimeApi";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import type {
  ProjectSettings,
  LlmConfig,
  PhaseControlPolicy,
  ConflictResolutionPolicy,
  AutonomyMode,
  ProjectRuntimeSpec,
  RuntimeReadinessCheck,
  RuntimeStopBehavior,
} from "../../types";
import { useAppStore } from "../../store/useAppStore";
import { useProjectStore } from "../../store/useProjectStore";
import { useToastStore } from "../../store/useToastStore";
import { useDialogStore } from "../../store/useDialogStore";
import { devLog } from "../../utils/devLog";

const PROVIDERS = ["claude", "openai"] as const;

interface KeyState {
  value: string;
  masked: string;
  loaded: boolean;
}

function createRuntimeSpecDraft(): ProjectRuntimeSpec {
  return {
    installCommand: null,
    runCommand: "",
    readinessCheck: { kind: "none" },
    verifyCommand: null,
    stopBehavior: { kind: "kill" },
    appUrl: null,
    portHint: null,
  };
}

function cloneRuntimeSpec(spec: ProjectRuntimeSpec): ProjectRuntimeSpec {
  return {
    ...spec,
    readinessCheck: { ...spec.readinessCheck },
    stopBehavior: { ...spec.stopBehavior },
  };
}

export function SettingsPage() {
  const goToProjects = useAppStore((s) => s.goToProjects);
  const activeProjectId = useAppStore((s) => s.activeProjectId);
  const project = useProjectStore((s) => s.project);
  const addToast = useToastStore((s) => s.addToast);
  const showConfirm = useDialogStore((s) => s.showConfirm);
  const currentGoalRun = useGoalRunStore((s) => s.currentGoalRun);
  const runtimeStatus = useGoalRunStore((s) => s.runtimeStatus);
  const runtimeLogs = useGoalRunStore((s) => s.runtimeLogs);
  const refreshRuntimeStatus = useGoalRunStore((s) => s.refreshRuntimeStatus);

  // API keys state
  const [keys, setKeys] = useState<Record<string, KeyState>>({});
  const [keyInputs, setKeyInputs] = useState<Record<string, string>>({});
  const [savingKey, setSavingKey] = useState<Record<string, boolean>>({});
  const [clearingKey, setClearingKey] = useState<Record<string, boolean>>({});

  // Project settings state
  const [savingProjectSettings, setSavingProjectSettings] = useState(false);
  const [workingDirectory, setWorkingDirectory] = useState("");
  const [workingDirValid, setWorkingDirValid] = useState<boolean | null>(null);
  const [workingDirError, setWorkingDirError] = useState("");
  const [tokenBudget, setTokenBudget] = useState(100_000);
  const [autonomyMode, setAutonomyMode] = useState<AutonomyMode>("autopilot");
  const [phaseControl, setPhaseControl] = useState<PhaseControlPolicy>("manual");
  const [llmConfigs, setLlmConfigs] = useState<LlmConfig[]>([]);
  const [defaultExecutionEngine, setDefaultExecutionEngine] = useState<string>("built-in");
  const [conflictResolution, setConflictResolution] = useState<ConflictResolutionPolicy>("ai-assisted");
  const [postRunValidationCommand, setPostRunValidationCommand] = useState("");
  const [runtimeSpecDraft, setRuntimeSpecDraft] = useState<ProjectRuntimeSpec | null>(null);

  // Load API keys
  useEffect(() => {
    for (const provider of PROVIDERS) {
      api.getApiKey(provider).then((key) => {
        if (key) {
          const masked = key.length > 4 ? "****" + key.slice(-4) : "****";
          setKeys((prev) => ({
            ...prev,
            [provider]: { value: key, masked, loaded: true },
          }));
        } else {
          setKeys((prev) => ({
            ...prev,
            [provider]: { value: "", masked: "", loaded: true },
          }));
        }
      }).catch(() => {
        setKeys((prev) => ({
          ...prev,
          [provider]: { value: "", masked: "", loaded: true },
        }));
      });
    }
  }, []);

  // Load project settings when project is available
  useEffect(() => {
    if (project) {
      setTokenBudget(project.settings.defaultTokenBudget);
      setAutonomyMode(project.settings.autonomyMode ?? "autopilot");
      setPhaseControl(project.settings.phaseControl);
      setLlmConfigs(project.settings.llmConfigs);
      setDefaultExecutionEngine(project.settings.defaultExecutionEngine ?? "built-in");
      setConflictResolution(project.settings.conflictResolution ?? "ai-assisted");
      setWorkingDirectory(project.settings.workingDirectory ?? "");
      setPostRunValidationCommand(project.settings.postRunValidationCommand ?? "");
      setRuntimeSpecDraft(
        project.settings.runtimeSpec ? cloneRuntimeSpec(project.settings.runtimeSpec) : null,
      );
      // Validate existing working directory
      if (project.settings.workingDirectory) {
        api.validateWorkingDirectory(project.settings.workingDirectory)
          .then(() => { setWorkingDirValid(true); setWorkingDirError(""); })
          .catch((e) => { setWorkingDirValid(false); setWorkingDirError(String(e)); });
      } else {
        setWorkingDirValid(null);
        setWorkingDirError("");
      }
    } else {
      setTokenBudget(100_000);
      setAutonomyMode("autopilot");
      setPhaseControl("manual");
      setLlmConfigs([]);
      setDefaultExecutionEngine("built-in");
      setConflictResolution("ai-assisted");
      setWorkingDirectory("");
      setPostRunValidationCommand("");
      setRuntimeSpecDraft(null);
      setWorkingDirValid(null);
      setWorkingDirError("");
    }
  }, [project]);

  useEffect(() => {
    if (!project?.id || activeProjectId !== project.id) return;
    void refreshRuntimeStatus(project.id);
  }, [activeProjectId, project?.id, refreshRuntimeStatus]);

  const handleSaveKey = async (provider: string) => {
    const value = keyInputs[provider];
    if (!value?.trim()) return;
    setSavingKey((prev) => ({ ...prev, [provider]: true }));
    try {
      await api.setApiKey(provider, value.trim());
      const masked = value.length > 4 ? "****" + value.slice(-4) : "****";
      setKeys((prev) => ({
        ...prev,
        [provider]: { value: value.trim(), masked, loaded: true },
      }));
      setKeyInputs((prev) => ({ ...prev, [provider]: "" }));
      devLog("info", "Settings", `API key saved for ${provider}`);
      addToast(`${provider} key saved`, "info");
    } catch (e) {
      devLog("error", "Settings", `Failed to save API key for ${provider}`, e);
      addToast(`Failed to save key: ${e}`);
    } finally {
      setSavingKey((prev) => ({ ...prev, [provider]: false }));
    }
  };

  const handleClearKey = async (provider: string) => {
    setClearingKey((prev) => ({ ...prev, [provider]: true }));
    try {
      await api.deleteApiKey(provider);
      setKeys((prev) => ({
        ...prev,
        [provider]: { value: "", masked: "", loaded: true },
      }));
      addToast(`${provider} key removed`, "info");
    } catch (e) {
      addToast(`Failed to remove key: ${e}`);
    } finally {
      setClearingKey((prev) => ({ ...prev, [provider]: false }));
    }
  };

  const persistProjectSettings = async (runtimeSpec: ProjectRuntimeSpec | null) => {
    if (!project || !activeProjectId || project.id !== activeProjectId) return;
    if (runtimeSpec && !runtimeSpec.runCommand.trim()) {
      addToast("Runtime run command is required before saving runtime settings", "warning");
      return;
    }
    setSavingProjectSettings(true);
    try {
      const settings: ProjectSettings = {
        defaultTokenBudget: tokenBudget,
        autonomyMode,
        phaseControl,
        conflictResolution,
        llmConfigs,
        workingDirectory: workingDirectory.trim() || null,
        defaultExecutionEngine: defaultExecutionEngine === "built-in" ? null : defaultExecutionEngine,
        postRunValidationCommand: postRunValidationCommand.trim() || null,
        runtimeSpec,
      };
      await api.updateProjectSettings(project.id, settings);
      await refreshRuntimeStatus(project.id).catch(() => undefined);
      devLog("info", "Settings", "Project settings saved", {
        autonomyMode,
        phaseControl,
        conflictResolution,
        workingDirectory,
      });
      addToast("Project settings saved", "info");
    } catch (e) {
      devLog("error", "Settings", "Failed to save project settings", e);
      addToast(`Failed to save settings: ${e}`);
    } finally {
      setSavingProjectSettings(false);
    }
  };

  const handleSaveProjectSettings = async () => {
    await persistProjectSettings(runtimeSpecDraft);
  };

  const updateLlmConfig = (index: number, field: keyof LlmConfig, value: string | null) => {
    setLlmConfigs((prev) =>
      prev.map((c, i) => (i === index ? { ...c, [field]: value } : c))
    );
  };

  const addLlmConfig = () => {
    setLlmConfigs((prev) => [
      ...prev,
      { provider: "claude", model: "claude-sonnet-4-6", apiKeyEnv: null, baseUrl: null },
    ]);
  };

  const removeLlmConfig = (index: number) => {
    setLlmConfigs((prev) => prev.filter((_, i) => i !== index));
  };

  const updateRuntimeSpec = (updater: (spec: ProjectRuntimeSpec) => ProjectRuntimeSpec) => {
    setRuntimeSpecDraft((current) => updater(current ? cloneRuntimeSpec(current) : createRuntimeSpecDraft()));
  };

  const handleDetectRuntime = async () => {
    if (!project) return;
    try {
      const detected = await runtimeApi.detectRuntime(project.id);
      if (!detected) {
        addToast("No runtime contract could be detected from the working directory", "warning");
        return;
      }
      setRuntimeSpecDraft(cloneRuntimeSpec(detected));
      addToast("Detected runtime contract from the repo", "info");
    } catch (e) {
      addToast(`Runtime detection failed: ${e}`);
    }
  };

  const handleClearRuntime = () => {
    showConfirm("Clear the runtime contract for this project?", async () => {
      setRuntimeSpecDraft(null);
      await persistProjectSettings(null);
    });
  };

  return (
    <div className="flex h-full flex-col bg-gray-950 text-gray-100">
      {/* Header */}
      <div className="flex items-center gap-3 border-b border-gray-800 bg-gray-900 px-6 py-3">
        <button
          onClick={goToProjects}
          className="rounded px-2 py-1 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200 transition-colors"
        >
          &larr; Back
        </button>
        <h1 className="text-lg font-semibold">Settings</h1>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="mx-auto max-w-2xl space-y-8">
          {/* API Keys Section */}
          <section>
            <h2 className="text-sm font-semibold text-gray-300 mb-3">
              API Keys
            </h2>
            <p className="text-xs text-gray-500 mb-4">
              Stored securely in your OS keychain.
            </p>
            <div className="space-y-3">
              {PROVIDERS.map((provider) => {
                const keyState = keys[provider];
                const hasKey = keyState?.value;
                return (
                  <div
                    key={provider}
                    className="flex items-center gap-3 rounded-lg border border-gray-800 bg-gray-900 px-4 py-3"
                  >
                    <span className="text-sm font-medium text-gray-300 w-20 capitalize">
                      {provider}
                    </span>
                    {hasKey ? (
                      <>
                        <span className="flex-1 text-sm text-gray-500 font-mono">
                          {keyState.masked}
                        </span>
                        <button
                          onClick={() =>
                            showConfirm(
                              `Remove the ${provider} API key?`,
                              () => handleClearKey(provider),
                            )
                          }
                          disabled={clearingKey[provider]}
                          className="rounded px-2.5 py-1 text-xs text-red-400 hover:bg-gray-800 hover:text-red-300 border border-gray-700 disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          {clearingKey[provider] ? "Clearing..." : "Clear"}
                        </button>
                      </>
                    ) : (
                      <>
                        <input
                          type="password"
                          value={keyInputs[provider] || ""}
                          onChange={(e) =>
                            setKeyInputs((prev) => ({
                              ...prev,
                              [provider]: e.target.value,
                            }))
                          }
                          onKeyDown={(e) => {
                            if (e.key === "Enter") handleSaveKey(provider);
                          }}
                          placeholder={`Enter ${provider} API key...`}
                          className="flex-1 rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 placeholder-gray-600 focus:border-blue-500 focus:outline-none"
                        />
                        <button
                          onClick={() => handleSaveKey(provider)}
                          disabled={savingKey[provider] || !keyInputs[provider]?.trim()}
                          className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          {savingKey[provider] ? "Saving..." : "Save"}
                        </button>
                      </>
                    )}
                  </div>
                );
              })}
            </div>
          </section>

          {/* Project Settings Section */}
          {activeProjectId && project && project.id === activeProjectId && (
            <section>
              <h2 className="text-sm font-semibold text-gray-300 mb-3">
                Project Settings
              </h2>
              <p className="text-xs text-gray-500 mb-4">
                Settings for "{project.name}"
              </p>
              <div className="space-y-4">
                {/* Token budget */}
                <div className="flex items-center gap-3">
                  <label className="text-sm text-gray-400 w-40">
                    Default response length
                  </label>
                  <input
                    type="number"
                    value={tokenBudget}
                    onChange={(e) => setTokenBudget(Number(e.target.value))}
                    className="w-32 rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none"
                  />
                </div>

                <div className="flex items-center gap-3">
                  <label className="text-sm text-gray-400 w-40">
                    Phase management
                  </label>
                  <select
                    value={phaseControl}
                    onChange={(e) =>
                      setPhaseControl(e.target.value as PhaseControlPolicy)
                    }
                    className="rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none"
                  >
                    <option value="manual">Manual</option>
                    <option value="gated-auto-advance">Auto-advance with approval</option>
                    <option value="fully-autonomous">Fully automatic</option>
                  </select>
                </div>

                {/* Conflict Resolution */}
                <div className="flex items-center gap-3">
                  <label className="text-sm text-gray-400 w-40">
                    Merge conflicts
                  </label>
                  <select
                    value={conflictResolution}
                    onChange={(e) =>
                      setConflictResolution(e.target.value as ConflictResolutionPolicy)
                    }
                    className="rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none"
                  >
                    <option value="manual">Manual (flag and stop)</option>
                    <option value="ai-assisted">AI-assisted (offer resolve button)</option>
                    <option value="auto-resolve">Auto-resolve (AI fixes silently)</option>
                  </select>
                </div>

                {/* Default Execution Engine */}
                <div className="flex items-center gap-3">
                  <label className="text-sm text-gray-400 w-40">
                    Default agent engine
                  </label>
                  <select
                    value={defaultExecutionEngine}
                    onChange={(e) => setDefaultExecutionEngine(e.target.value)}
                    className="rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none"
                  >
                    <option value="built-in">Built-in LLM (text only)</option>
                    <option value="claude-code">Claude Code (writes files)</option>
                    <option value="codex">Codex (writes files)</option>
                  </select>
                </div>

                {/* Working Directory */}
                <div>
                  <label className="block text-sm text-gray-400 mb-1">
                    Working Directory
                  </label>
                  <p className="text-[10px] text-gray-600 mb-1.5">
                    Git repo path for external tools (Claude Code, Codex).
                  </p>
                  <div className="flex items-center gap-2">
                    <input
                      type="text"
                      value={workingDirectory}
                      onChange={(e) => {
                        setWorkingDirectory(e.target.value);
                        setWorkingDirValid(null);
                        setWorkingDirError("");
                      }}
                      onBlur={() => {
                        if (workingDirectory.trim()) {
                          api.validateWorkingDirectory(workingDirectory.trim())
                            .then(() => { setWorkingDirValid(true); setWorkingDirError(""); })
                            .catch((e) => { setWorkingDirValid(false); setWorkingDirError(String(e)); });
                        } else {
                          setWorkingDirValid(null);
                          setWorkingDirError("");
                        }
                      }}
                      placeholder="/path/to/your/repo"
                      className="flex-1 rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 placeholder-gray-600 focus:border-blue-500 focus:outline-none font-mono"
                    />
                    <button
                      onClick={async () => {
                        try {
                          const { open } = await import("@tauri-apps/plugin-dialog");
                          const selected = await open({ directory: true, multiple: false });
                          if (selected && typeof selected === "string") {
                            setWorkingDirectory(selected);
                            api.validateWorkingDirectory(selected)
                              .then(() => { setWorkingDirValid(true); setWorkingDirError(""); })
                              .catch((e) => { setWorkingDirValid(false); setWorkingDirError(String(e)); });
                          }
                        } catch (e) {
                          addToast(`Browse failed: ${e}`);
                        }
                      }}
                      className="rounded border border-gray-700 px-2.5 py-1 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200"
                    >
                      Browse
                    </button>
                    {workingDirValid === true && (
                      <span className="text-green-400 text-xs" title="Valid git repo">&#10003;</span>
                    )}
                    {workingDirValid === false && (
                      <span className="text-red-400 text-xs" title={workingDirError}>&#10007;</span>
                    )}
                  </div>
                  {workingDirError && (
                    <p className="text-[10px] text-red-400 mt-1">{workingDirError}</p>
                  )}
                </div>

                <div>
                  <label className="block text-sm text-gray-400 mb-1">
                    Post-run validation command
                  </label>
                  <p className="text-[10px] text-gray-600 mb-1.5">
                    Optional shell command run in the working directory after successful external implementation runs.
                  </p>
                  <input
                    type="text"
                    value={postRunValidationCommand}
                    onChange={(e) => setPostRunValidationCommand(e.target.value)}
                    placeholder="npm test"
                    className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 placeholder-gray-600 focus:border-blue-500 focus:outline-none font-mono"
                  />
                </div>

                {/* Runtime contract */}
                <div className="rounded-lg border border-gray-800 bg-gray-900/80 p-4 space-y-4">
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <h3 className="text-sm font-semibold text-gray-300">Runtime contract</h3>
                      <p className="text-[10px] text-gray-500">
                        Local-process settings used to run and verify the generated app.
                      </p>
                    </div>
                    <div className="flex gap-2">
                      <button
                        onClick={handleDetectRuntime}
                        className="rounded border border-gray-700 px-2 py-1 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200"
                      >
                        Detect
                      </button>
                      <button
                        onClick={handleClearRuntime}
                        className="rounded border border-gray-700 px-2 py-1 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200"
                      >
                        Clear
                      </button>
                    </div>
                  </div>

                  {!runtimeSpecDraft ? (
                    <div className="rounded border border-dashed border-gray-700 bg-gray-950/60 px-3 py-3 text-xs text-gray-500">
                      No runtime contract is configured yet.
                      <div className="mt-3">
                        <button
                          onClick={() => setRuntimeSpecDraft(createRuntimeSpecDraft())}
                          className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-500"
                        >
                          Initialize runtime config
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="space-y-3">
                      <div className="grid gap-3 md:grid-cols-2">
                        <label className="space-y-1">
                          <span className="block text-xs text-gray-400">Install command</span>
                          <input
                            type="text"
                            value={runtimeSpecDraft.installCommand ?? ""}
                            onChange={(e) =>
                              updateRuntimeSpec((spec) => ({
                                ...spec,
                                installCommand: e.target.value.trim() ? e.target.value : null,
                              }))
                            }
                            placeholder="npm install"
                            className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 placeholder-gray-600 focus:border-blue-500 focus:outline-none font-mono"
                          />
                        </label>
                        <label className="space-y-1">
                          <span className="block text-xs text-gray-400">Run command</span>
                          <input
                            type="text"
                            value={runtimeSpecDraft.runCommand}
                            onChange={(e) =>
                              updateRuntimeSpec((spec) => ({
                                ...spec,
                                runCommand: e.target.value,
                              }))
                            }
                            placeholder="npm run dev"
                            className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 placeholder-gray-600 focus:border-blue-500 focus:outline-none font-mono"
                          />
                        </label>
                        <label className="space-y-1">
                          <span className="block text-xs text-gray-400">Verify command</span>
                          <input
                            type="text"
                            value={runtimeSpecDraft.verifyCommand ?? ""}
                            onChange={(e) =>
                              updateRuntimeSpec((spec) => ({
                                ...spec,
                                verifyCommand: e.target.value.trim() ? e.target.value : null,
                              }))
                            }
                            placeholder="npm test"
                            className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 placeholder-gray-600 focus:border-blue-500 focus:outline-none font-mono"
                          />
                        </label>
                        <label className="space-y-1">
                          <span className="block text-xs text-gray-400">App URL</span>
                          <input
                            type="text"
                            value={runtimeSpecDraft.appUrl ?? ""}
                            onChange={(e) =>
                              updateRuntimeSpec((spec) => ({
                                ...spec,
                                appUrl: e.target.value.trim() ? e.target.value : null,
                              }))
                            }
                            placeholder="http://127.0.0.1:3000"
                            className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 placeholder-gray-600 focus:border-blue-500 focus:outline-none font-mono"
                          />
                        </label>
                        <label className="space-y-1">
                          <span className="block text-xs text-gray-400">Port hint</span>
                          <input
                            type="number"
                            value={runtimeSpecDraft.portHint ?? ""}
                            onChange={(e) =>
                              updateRuntimeSpec((spec) => ({
                                ...spec,
                                portHint: e.target.value.trim() ? Number(e.target.value) : null,
                              }))
                            }
                            placeholder="3000"
                            className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 placeholder-gray-600 focus:border-blue-500 focus:outline-none font-mono"
                          />
                        </label>
                      </div>

                      <div className="grid gap-3 md:grid-cols-2">
                        <label className="space-y-1">
                          <span className="block text-xs text-gray-400">Readiness check</span>
                          <select
                            value={runtimeSpecDraft.readinessCheck.kind}
                            onChange={(e) => {
                              const kind = e.target.value as RuntimeReadinessCheck["kind"];
                              updateRuntimeSpec((spec) => ({
                                ...spec,
                                readinessCheck:
                                  kind === "http"
                                    ? {
                                        kind,
                                        path: "/",
                                        expectedStatus: 200,
                                        timeoutSeconds: 30,
                                        pollIntervalMs: 500,
                                      }
                                    : kind === "tcpPort"
                                      ? {
                                          kind,
                                          timeoutSeconds: 30,
                                          pollIntervalMs: 500,
                                        }
                                      : { kind: "none" },
                              }));
                            }}
                            className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none"
                          >
                            <option value="none">None</option>
                            <option value="http">HTTP</option>
                            <option value="tcpPort">TCP port</option>
                          </select>
                        </label>
                        <label className="space-y-1">
                          <span className="block text-xs text-gray-400">Stop behavior</span>
                          <select
                            value={runtimeSpecDraft.stopBehavior.kind}
                            onChange={(e) => {
                              const kind = e.target.value as RuntimeStopBehavior["kind"];
                              updateRuntimeSpec((spec) => ({
                                ...spec,
                                stopBehavior:
                                  kind === "graceful"
                                    ? { kind, timeoutSeconds: 5 }
                                    : { kind: "kill" },
                              }));
                            }}
                            className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none"
                          >
                            <option value="kill">Kill immediately</option>
                            <option value="graceful">Graceful shutdown</option>
                          </select>
                        </label>
                      </div>

                      {runtimeSpecDraft.readinessCheck.kind === "http" && (
                        <div className="grid gap-3 md:grid-cols-3">
                          <label className="space-y-1">
                            <span className="block text-xs text-gray-400">HTTP path</span>
                            <input
                              type="text"
                              value={runtimeSpecDraft.readinessCheck.path ?? "/"}
                              onChange={(e) =>
                                updateRuntimeSpec((spec) => ({
                                  ...spec,
                                  readinessCheck: {
                                    ...spec.readinessCheck,
                                    kind: "http",
                                    path: e.target.value,
                                  },
                                }))
                              }
                              className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none font-mono"
                            />
                          </label>
                          <label className="space-y-1">
                            <span className="block text-xs text-gray-400">Expected status</span>
                            <input
                              type="number"
                              value={runtimeSpecDraft.readinessCheck.expectedStatus ?? 200}
                              onChange={(e) =>
                                updateRuntimeSpec((spec) => ({
                                  ...spec,
                                  readinessCheck: {
                                    ...spec.readinessCheck,
                                    kind: "http",
                                    expectedStatus: Number(e.target.value),
                                  },
                                }))
                              }
                              className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none font-mono"
                            />
                          </label>
                          <label className="space-y-1">
                            <span className="block text-xs text-gray-400">Timeout seconds</span>
                            <input
                              type="number"
                              value={runtimeSpecDraft.readinessCheck.timeoutSeconds ?? 30}
                              onChange={(e) =>
                                updateRuntimeSpec((spec) => ({
                                  ...spec,
                                  readinessCheck: {
                                    ...spec.readinessCheck,
                                    kind: "http",
                                    timeoutSeconds: Number(e.target.value),
                                  },
                                }))
                              }
                              className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none font-mono"
                            />
                          </label>
                        </div>
                      )}

                      {runtimeSpecDraft.readinessCheck.kind === "tcpPort" && (
                        <div className="grid gap-3 md:grid-cols-2">
                          <label className="space-y-1">
                            <span className="block text-xs text-gray-400">Timeout seconds</span>
                            <input
                              type="number"
                              value={runtimeSpecDraft.readinessCheck.timeoutSeconds ?? 30}
                              onChange={(e) =>
                                updateRuntimeSpec((spec) => ({
                                  ...spec,
                                  readinessCheck: {
                                    ...spec.readinessCheck,
                                    kind: "tcpPort",
                                    timeoutSeconds: Number(e.target.value),
                                  },
                                }))
                              }
                              className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none font-mono"
                            />
                          </label>
                          <label className="space-y-1">
                            <span className="block text-xs text-gray-400">Poll interval ms</span>
                            <input
                              type="number"
                              value={runtimeSpecDraft.readinessCheck.pollIntervalMs ?? 500}
                              onChange={(e) =>
                                updateRuntimeSpec((spec) => ({
                                  ...spec,
                                  readinessCheck: {
                                    ...spec.readinessCheck,
                                    kind: "tcpPort",
                                    pollIntervalMs: Number(e.target.value),
                                  },
                                }))
                              }
                              className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none font-mono"
                            />
                          </label>
                        </div>
                      )}

                      {runtimeSpecDraft.stopBehavior.kind === "graceful" && (
                        <div className="grid gap-3 md:grid-cols-2">
                          <label className="space-y-1">
                            <span className="block text-xs text-gray-400">Grace timeout seconds</span>
                            <input
                              type="number"
                              value={runtimeSpecDraft.stopBehavior.timeoutSeconds ?? 5}
                              onChange={(e) =>
                                updateRuntimeSpec((spec) => ({
                                  ...spec,
                                  stopBehavior: {
                                    kind: "graceful",
                                    timeoutSeconds: Number(e.target.value),
                                  },
                                }))
                              }
                              className="w-full rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none font-mono"
                            />
                          </label>
                        </div>
                      )}
                    </div>
                  )}
                </div>

                {/* Runtime status preview */}
                <div className="rounded-lg border border-gray-800 bg-gray-900/70 p-4 space-y-3">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <h3 className="text-sm font-semibold text-gray-300">Runtime status</h3>
                      <p className="text-[10px] text-gray-500">
                        Live runtime state from the active goal-run session.
                      </p>
                    </div>
                    <button
                      onClick={() => project && refreshRuntimeStatus(project.id)}
                      className="rounded border border-gray-700 px-2 py-1 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200"
                    >
                      Refresh
                    </button>
                  </div>
                  <div className="flex flex-wrap gap-2 text-[10px]">
                    <span className="rounded border border-gray-700 bg-gray-950 px-2 py-1 text-gray-300">
                      Autonomy: {autonomyMode}
                    </span>
                    <span className="rounded border border-gray-700 bg-gray-950 px-2 py-1 text-gray-300">
                      Goal run: {currentGoalRun ? `${currentGoalRun.status} / ${currentGoalRun.phase}` : "none"}
                    </span>
                    <span className="rounded border border-gray-700 bg-gray-950 px-2 py-1 text-gray-300">
                      Runtime: {runtimeStatus?.session?.status ?? "idle"}
                    </span>
                  </div>
                  {runtimeStatus?.session?.url && (
                    <p className="text-xs text-gray-400 font-mono break-all">
                      {runtimeStatus.session.url}
                    </p>
                  )}
                  {runtimeLogs.length > 0 && (
                    <pre className="max-h-40 overflow-y-auto rounded border border-gray-800 bg-gray-950 px-3 py-2 text-[10px] text-gray-300 whitespace-pre-wrap">
                      {runtimeLogs.join("\n")}
                    </pre>
                  )}
                </div>

                {/* LLM Configs */}
                <div>
                  <div className="flex items-center justify-between mb-2">
                    <label className="text-sm text-gray-400">
                      AI Model Settings
                    </label>
                    <button
                      onClick={addLlmConfig}
                      className="rounded border border-gray-700 px-2 py-0.5 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200"
                    >
                      + Add
                    </button>
                  </div>
                  {llmConfigs.length === 0 && (
                    <p className="text-xs text-gray-600">
                      No custom AI models configured. Uses default settings.
                    </p>
                  )}
                  {llmConfigs.map((config, i) => (
                    <div
                      key={i}
                      className="flex items-center gap-2 mb-2 rounded border border-gray-800 bg-gray-900 px-3 py-2"
                    >
                      <input
                        value={config.provider}
                        onChange={(e) =>
                          updateLlmConfig(i, "provider", e.target.value)
                        }
                        placeholder="Provider"
                        className="w-24 rounded border border-gray-700 bg-gray-800 px-2 py-0.5 text-xs text-gray-200 focus:border-blue-500 focus:outline-none"
                      />
                      <input
                        value={config.model}
                        onChange={(e) =>
                          updateLlmConfig(i, "model", e.target.value)
                        }
                        placeholder="Model"
                        className="flex-1 rounded border border-gray-700 bg-gray-800 px-2 py-0.5 text-xs text-gray-200 focus:border-blue-500 focus:outline-none"
                      />
                      <input
                        value={config.baseUrl || ""}
                        onChange={(e) =>
                          updateLlmConfig(
                            i,
                            "baseUrl",
                            e.target.value || null,
                          )
                        }
                        placeholder="Base URL (optional)"
                        className="w-48 rounded border border-gray-700 bg-gray-800 px-2 py-0.5 text-xs text-gray-200 focus:border-blue-500 focus:outline-none"
                      />
                      <button
                        onClick={() => removeLlmConfig(i)}
                        className="text-xs text-red-400 hover:text-red-300 px-1"
                      >
                        x
                      </button>
                    </div>
                  ))}
                </div>

                <button
                  onClick={handleSaveProjectSettings}
                  disabled={savingProjectSettings}
                  className="rounded bg-blue-600 px-4 py-1.5 text-xs font-medium text-white hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  {savingProjectSettings ? "Saving..." : "Save Project Settings"}
                </button>
              </div>
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
