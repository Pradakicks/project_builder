import { useState, useEffect } from "react";
import * as api from "../../api/tauriApi";
import type { ProjectSettings, LlmConfig, PhaseControlPolicy } from "../../types";
import { useAppStore } from "../../store/useAppStore";
import { useProjectStore } from "../../store/useProjectStore";
import { useToastStore } from "../../store/useToastStore";

const PROVIDERS = ["claude", "openai"] as const;

interface KeyState {
  value: string;
  masked: string;
  loaded: boolean;
}

export function SettingsPage() {
  const goToProjects = useAppStore((s) => s.goToProjects);
  const activeProjectId = useAppStore((s) => s.activeProjectId);
  const project = useProjectStore((s) => s.project);
  const addToast = useToastStore((s) => s.addToast);

  // API keys state
  const [keys, setKeys] = useState<Record<string, KeyState>>({});
  const [keyInputs, setKeyInputs] = useState<Record<string, string>>({});

  // Project settings state
  const [tokenBudget, setTokenBudget] = useState(100_000);
  const [phaseControl, setPhaseControl] = useState<PhaseControlPolicy>("manual");
  const [llmConfigs, setLlmConfigs] = useState<LlmConfig[]>([]);

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
      setPhaseControl(project.settings.phaseControl);
      setLlmConfigs(project.settings.llmConfigs);
    }
  }, [project]);

  const handleSaveKey = async (provider: string) => {
    const value = keyInputs[provider];
    if (!value?.trim()) return;
    try {
      await api.setApiKey(provider, value.trim());
      const masked = value.length > 4 ? "****" + value.slice(-4) : "****";
      setKeys((prev) => ({
        ...prev,
        [provider]: { value: value.trim(), masked, loaded: true },
      }));
      setKeyInputs((prev) => ({ ...prev, [provider]: "" }));
      addToast(`${provider} key saved`, "info");
    } catch (e) {
      addToast(`Failed to save key: ${e}`);
    }
  };

  const handleClearKey = async (provider: string) => {
    try {
      await api.deleteApiKey(provider);
      setKeys((prev) => ({
        ...prev,
        [provider]: { value: "", masked: "", loaded: true },
      }));
      addToast(`${provider} key removed`, "info");
    } catch (e) {
      addToast(`Failed to remove key: ${e}`);
    }
  };

  const handleSaveProjectSettings = async () => {
    if (!activeProjectId) return;
    try {
      const settings: ProjectSettings = {
        defaultTokenBudget: tokenBudget,
        phaseControl,
        llmConfigs,
      };
      await api.updateProjectSettings(activeProjectId, settings);
      addToast("Project settings saved", "info");
    } catch (e) {
      addToast(`Failed to save settings: ${e}`);
    }
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
                          onClick={() => handleClearKey(provider)}
                          className="rounded px-2.5 py-1 text-xs text-red-400 hover:bg-gray-800 hover:text-red-300 border border-gray-700"
                        >
                          Clear
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
                          className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-500"
                        >
                          Save
                        </button>
                      </>
                    )}
                  </div>
                );
              })}
            </div>
          </section>

          {/* Project Settings Section */}
          {activeProjectId && project && (
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
                    Default token budget
                  </label>
                  <input
                    type="number"
                    value={tokenBudget}
                    onChange={(e) => setTokenBudget(Number(e.target.value))}
                    className="w-32 rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none"
                  />
                </div>

                {/* Phase control */}
                <div className="flex items-center gap-3">
                  <label className="text-sm text-gray-400 w-40">
                    Phase control
                  </label>
                  <select
                    value={phaseControl}
                    onChange={(e) =>
                      setPhaseControl(e.target.value as PhaseControlPolicy)
                    }
                    className="rounded border border-gray-700 bg-gray-800 px-2.5 py-1 text-sm text-gray-200 focus:border-blue-500 focus:outline-none"
                  >
                    <option value="manual">Manual</option>
                    <option value="gated-auto-advance">Gated auto-advance</option>
                    <option value="fully-autonomous">Fully autonomous</option>
                  </select>
                </div>

                {/* LLM Configs */}
                <div>
                  <div className="flex items-center justify-between mb-2">
                    <label className="text-sm text-gray-400">
                      LLM configurations
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
                      No LLM configs. Uses defaults (Claude Sonnet).
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
                  className="rounded bg-blue-600 px-4 py-1.5 text-xs font-medium text-white hover:bg-blue-500 transition-colors"
                >
                  Save Project Settings
                </button>
              </div>
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
