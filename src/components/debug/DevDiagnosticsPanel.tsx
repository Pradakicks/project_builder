import { useEffect, useMemo, useState } from "react";
import { useAppStore } from "../../store/useAppStore";
import { useProjectStore } from "../../store/useProjectStore";
import { useToastStore } from "../../store/useToastStore";
import { useDebugStore } from "../../store/useDebugStore";
import { getDebugSessionInfo, getLastDebugScenario, readDebugLogTail } from "../../api/debugApi";
import { devLog } from "../../utils/devLog";

const isDev = import.meta.env.DEV;

export function DevDiagnosticsPanel() {
  const activeView = useAppStore((s) => s.view);
  const activeProjectId = useAppStore((s) => s.activeProjectId);
  const openProject = useAppStore((s) => s.openProject);
  const project = useProjectStore((s) => s.project);
  const events = useDebugStore((s) => s.events);
  const session = useDebugStore((s) => s.session);
  const lastScenario = useDebugStore((s) => s.lastScenario);
  const diagnosticsOpen = useDebugStore((s) => s.diagnosticsOpen);
  const replayHandler = useDebugStore((s) => s.replayHandler);
  const setSession = useDebugStore((s) => s.setSession);
  const setDiagnosticsOpen = useDebugStore((s) => s.setDiagnosticsOpen);
  const captureScenario = useDebugStore((s) => s.captureScenario);
  const buildReport = useDebugStore((s) => s.buildReport);
  const [logTail, setLogTail] = useState<string[]>([]);
  const [loadingLog, setLoadingLog] = useState(false);

  const visibleEvents = useMemo(() => events.slice(-25).reverse(), [events]);

  useEffect(() => {
    if (!isDev) return;
    void getDebugSessionInfo()
      .then(setSession)
      .catch((error) => devLog("warn", "Diagnostics", "Failed to load debug session info", error));
    void getLastDebugScenario()
      .then((scenario) => {
        if (scenario) captureScenario(scenario);
      })
      .catch((error) => devLog("warn", "Diagnostics", "Failed to load last debug scenario", error));
  }, [captureScenario, setSession]);

  useEffect(() => {
    if (!isDev || !diagnosticsOpen || !session?.enabled) {
      return;
    }

    setLoadingLog(true);
    void readDebugLogTail(120)
      .then((tail) => setLogTail(tail.lines))
      .catch((error) => devLog("warn", "Diagnostics", "Failed to read debug log tail", error))
      .finally(() => setLoadingLog(false));
  }, [diagnosticsOpen, session?.enabled]);

  if (!isDev) {
    return null;
  }

  const [copying, setCopying] = useState(false);
  const copyReport = async () => {
    setCopying(true);
    try {
      const report = await buildReport(activeProjectId, activeView);
      await navigator.clipboard.writeText(JSON.stringify(report, null, 2));
      useToastStore.getState().addToast("Copied debug report", "info");
    } catch (error) {
      useToastStore.getState().addToast(`Failed to copy debug report: ${error}`, "warning");
    } finally {
      setCopying(false);
    }
  };

  const replayLastScenario = async () => {
    if (!lastScenario) {
      useToastStore.getState().addToast("No captured scenario to replay", "warning");
      return;
    }
    if (lastScenario.projectId !== activeProjectId) {
      openProject(lastScenario.projectId);
      useToastStore.getState().addToast(
        "Opened the scenario project. Replay it once the project finishes loading.",
        "info",
      );
      return;
    }
    if (!replayHandler) {
      useToastStore.getState().addToast("The CTO panel is not ready to replay yet", "warning");
      return;
    }

    try {
      await replayHandler(lastScenario);
      useToastStore.getState().addToast("Replayed captured CTO scenario", "info");
    } catch (error) {
      useToastStore.getState().addToast(`Replay failed: ${error}`, "warning");
    }
  };

  return (
    <>
      <button
        onClick={() => setDiagnosticsOpen(!diagnosticsOpen)}
        className="fixed bottom-4 right-4 z-50 rounded-full border border-amber-600 bg-amber-950/90 px-3 py-2 text-[11px] font-semibold text-amber-100 shadow-lg hover:bg-amber-900"
        title="Open development diagnostics"
      >
        Dev Diagnostics
      </button>
      {diagnosticsOpen ? (
        <div className="fixed bottom-16 right-4 z-50 flex h-[70vh] w-[28rem] flex-col overflow-hidden rounded-xl border border-gray-700 bg-gray-950 text-gray-100 shadow-2xl">
          <div className="flex items-center justify-between border-b border-gray-800 px-4 py-3">
            <div>
              <p className="text-sm font-semibold">Developer Diagnostics</p>
              <p className="text-[11px] text-gray-400">
                Active view: {activeView}
                {project ? ` • ${project.name}` : ""}
              </p>
            </div>
            <button
              onClick={() => setDiagnosticsOpen(false)}
              className="text-xs text-gray-500 hover:text-gray-200"
            >
              Close
            </button>
          </div>

          <div className="grid grid-cols-2 gap-2 border-b border-gray-800 px-4 py-3 text-[11px] text-gray-300">
            <button
              onClick={() => void copyReport()}
              disabled={copying}
              className="rounded border border-gray-700 px-2 py-1 hover:bg-gray-900 disabled:opacity-50"
            >
              {copying ? "Collecting…" : "Copy Debug Report"}
            </button>
            <button
              onClick={() => void replayLastScenario()}
              className="rounded border border-gray-700 px-2 py-1 hover:bg-gray-900"
            >
              Replay Last Scenario
            </button>
          </div>

          <div className="overflow-y-auto px-4 py-3 text-[11px]">
            <section className="mb-4 space-y-1">
              <p className="font-semibold text-gray-200">Session</p>
              <p className="text-gray-400">
                {session?.enabled
                  ? `Session ${session.sessionId ?? "unknown"}`
                  : "No dev session runner detected"}
              </p>
              {session?.sessionDir ? <p className="break-all text-gray-500">{session.sessionDir}</p> : null}
              {session?.logPath ? <p className="break-all text-gray-500">Log: {session.logPath}</p> : null}
            </section>

            <section className="mb-4 space-y-2">
              <div className="flex items-center justify-between">
                <p className="font-semibold text-gray-200">Captured Scenario</p>
                {lastScenario?.status ? (
                  <span className="rounded bg-red-950/60 px-2 py-0.5 text-[10px] text-red-200">
                    {lastScenario.status}
                  </span>
                ) : null}
              </div>
              {lastScenario ? (
                <div className="rounded border border-gray-800 bg-gray-900/70 p-2 text-gray-300">
                  <p>Prompt: {lastScenario.prompt}</p>
                  {lastScenario.error ? <p className="mt-1 text-red-300">Error: {lastScenario.error}</p> : null}
                  {lastScenario.path ? (
                    <p className="mt-1 break-all text-gray-500">Artifact: {lastScenario.path}</p>
                  ) : null}
                </div>
              ) : (
                <p className="text-gray-500">No failure has been captured yet in this session.</p>
              )}
            </section>

            <section className="mb-4 space-y-2">
              <p className="font-semibold text-gray-200">Recent Events</p>
              <div className="space-y-1">
                {visibleEvents.map((event) => (
                  <div key={event.id} className="rounded border border-gray-800 bg-gray-900/70 p-2">
                    <p className="text-[10px] uppercase tracking-wide text-gray-500">
                      {event.kind} • {event.level} • {new Date(event.timestamp).toLocaleTimeString()}
                    </p>
                    <p className="mt-1 text-gray-200">{event.message}</p>
                    {event.data !== undefined ? (
                      <pre className="mt-1 overflow-x-auto whitespace-pre-wrap break-words text-[10px] text-gray-400">
                        {JSON.stringify(event.data, null, 2)}
                      </pre>
                    ) : null}
                  </div>
                ))}
              </div>
            </section>

            <section className="space-y-2">
              <p className="font-semibold text-gray-200">Debug Log Tail</p>
              {loadingLog ? (
                <p className="text-gray-500">Loading log tail…</p>
              ) : logTail.length > 0 ? (
                <pre className="max-h-72 overflow-auto rounded border border-gray-800 bg-black/40 p-2 text-[10px] text-gray-300">
                  {logTail.join("\n")}
                </pre>
              ) : (
                <p className="text-gray-500">
                  {session?.enabled
                    ? "No log lines captured yet."
                    : "Launch the app with the dev session runner to capture terminal logs here."}
                </p>
              )}
            </section>
          </div>
        </div>
      ) : null}
    </>
  );
}
