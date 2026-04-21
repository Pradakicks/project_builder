import { useCallback, useEffect, useMemo, useState } from "react";
import { useProjectStore } from "../../store/useProjectStore";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import * as goalRunApi from "../../api/goalRunApi";
import type { GoalRun, GoalRunEvent, GoalRunEventKind } from "../../types";
import { devLog } from "../../utils/devLog";

/// Time-ordered feed of goal-run events across every run in the active
/// project, enriched with phase-progress breadcrumbs. This is the cross-run
/// view that the phase-arc timeline can't give you (which is per-run).

interface FeedEntry {
  id: string;
  goalRunId: string;
  goalRunPrompt: string;
  phase: string;
  kind: GoalRunEventKind;
  summary: string;
  payloadJson: string | null;
  createdAt: string;
}

const KIND_COLOR: Record<string, string> = {
  "phase-started": "text-blue-300 bg-blue-950/30 border-blue-900/50",
  "phase-completed": "text-green-300 bg-green-950/30 border-green-900/50",
  failed: "text-red-300 bg-red-950/30 border-red-900/50",
  blocked: "text-red-300 bg-red-950/30 border-red-900/50",
  "retry-scheduled": "text-amber-300 bg-amber-950/30 border-amber-900/50",
  stopped: "text-gray-300 bg-gray-900/60 border-gray-800",
  paused: "text-yellow-300 bg-yellow-950/30 border-yellow-900/50",
  resumed: "text-emerald-300 bg-emerald-950/30 border-emerald-900/50",
  "cancelled-mid-phase": "text-red-300 bg-red-950/30 border-red-900/50",
  "heartbeat-stale": "text-orange-300 bg-orange-950/30 border-orange-900/50",
  note: "text-gray-300 bg-gray-900/60 border-gray-800",
};

function kindBadge(kind: GoalRunEventKind): string {
  return KIND_COLOR[kind] ?? "text-gray-300 bg-gray-900/60 border-gray-800";
}

function formatClock(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return iso;
  }
}

function formatDay(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  } catch {
    return iso;
  }
}

function groupByDay(entries: FeedEntry[]): Array<{ day: string; items: FeedEntry[] }> {
  const groups = new Map<string, FeedEntry[]>();
  for (const entry of entries) {
    const day = formatDay(entry.createdAt);
    const bucket = groups.get(day);
    if (bucket) bucket.push(entry);
    else groups.set(day, [entry]);
  }
  return Array.from(groups.entries()).map(([day, items]) => ({ day, items }));
}

export function ActivityFeed() {
  const project = useProjectStore((s) => s.project);
  const goalRuns = useGoalRunStore((s) => s.goalRuns);
  const phaseActivity = useGoalRunStore((s) => s.phaseActivity);

  const [entries, setEntries] = useState<FeedEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [phaseFilter, setPhaseFilter] = useState<string>("all");
  const [kindFilter, setKindFilter] = useState<string>("all");
  const [search, setSearch] = useState("");

  const load = useCallback(async (runs: GoalRun[]) => {
    if (runs.length === 0) {
      setEntries([]);
      return;
    }
    setLoading(true);
    try {
      const all: FeedEntry[] = [];
      for (const run of runs.slice(0, 10)) {
        // Common case is 1–3 active runs; capping at 10 avoids a fat initial
        // fetch on projects with heavy history without needing a new command.
        const events = await goalRunApi.getGoalRunEvents(run.id).catch(() => [] as GoalRunEvent[]);
        for (const e of events) {
          all.push({
            id: e.id,
            goalRunId: e.goalRunId,
            goalRunPrompt: run.prompt,
            phase: e.phase,
            kind: e.kind,
            summary: e.summary,
            payloadJson: e.payloadJson,
            createdAt: e.createdAt,
          });
        }
      }
      all.sort((a, b) => b.createdAt.localeCompare(a.createdAt));
      setEntries(all);
    } catch (error) {
      devLog("warn", "ActivityFeed", "Failed to load events", error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load(goalRuns);
  }, [load, goalRuns]);

  // Live refresh every 5s while mounted. Phase-progress already updates the
  // `phaseActivity` banner immediately; this poll backstops it by reconciling
  // the full feed from persistent events.
  useEffect(() => {
    const id = window.setInterval(() => void load(goalRuns), 5000);
    return () => window.clearInterval(id);
  }, [load, goalRuns]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return entries.filter((e) => {
      if (phaseFilter !== "all" && e.phase !== phaseFilter) return false;
      if (kindFilter !== "all" && e.kind !== kindFilter) return false;
      if (q && !e.summary.toLowerCase().includes(q) && !e.goalRunPrompt.toLowerCase().includes(q)) {
        return false;
      }
      return true;
    });
  }, [entries, phaseFilter, kindFilter, search]);

  const phaseOptions = useMemo(() => {
    const set = new Set<string>();
    for (const e of entries) set.add(e.phase);
    return ["all", ...Array.from(set).sort()];
  }, [entries]);

  const kindOptions = useMemo(() => {
    const set = new Set<string>();
    for (const e of entries) set.add(e.kind);
    return ["all", ...Array.from(set).sort()];
  }, [entries]);

  const grouped = useMemo(() => groupByDay(filtered), [filtered]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 border-b border-gray-800 px-3 py-2">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.16em] text-emerald-300">
              Activity
            </p>
            <p className="text-[10px] text-gray-500">
              {project ? `${project.name} · ${entries.length} events` : "No project"}
            </p>
          </div>
          <button
            onClick={() => void load(goalRuns)}
            disabled={loading}
            className="rounded border border-gray-700 px-2 py-1 text-[10px] text-gray-400 hover:bg-gray-800 hover:text-gray-200 disabled:opacity-50"
          >
            {loading ? "Loading…" : "Refresh"}
          </button>
        </div>

        {phaseActivity && (
          <div className="mt-2 rounded border border-cyan-900/50 bg-cyan-950/20 px-2 py-1.5 text-[11px] text-cyan-100">
            <span className="font-medium">
              {phaseActivity.phase}
              {phaseActivity.stepIndex != null && phaseActivity.stepTotal != null
                ? ` (${phaseActivity.stepIndex}/${phaseActivity.stepTotal})`
                : ""}
            </span>
            <span className="ml-2 text-cyan-200/80">{phaseActivity.message}</span>
          </div>
        )}

        <div className="mt-2 flex flex-wrap gap-1.5 text-[10px]">
          <select
            value={phaseFilter}
            onChange={(e) => setPhaseFilter(e.target.value)}
            className="rounded border border-gray-700 bg-gray-900 px-1.5 py-0.5 text-[10px] text-gray-200"
          >
            {phaseOptions.map((p) => (
              <option key={p} value={p}>
                phase: {p}
              </option>
            ))}
          </select>
          <select
            value={kindFilter}
            onChange={(e) => setKindFilter(e.target.value)}
            className="rounded border border-gray-700 bg-gray-900 px-1.5 py-0.5 text-[10px] text-gray-200"
          >
            {kindOptions.map((k) => (
              <option key={k} value={k}>
                kind: {k}
              </option>
            ))}
          </select>
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="search…"
            className="flex-1 rounded border border-gray-700 bg-gray-900 px-1.5 py-0.5 text-[10px] text-gray-200 placeholder:text-gray-600"
          />
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto px-3 py-2">
        {grouped.length === 0 && !loading && (
          <p className="text-[11px] text-gray-500">
            No activity yet. Events will show up here as goal runs progress.
          </p>
        )}
        {grouped.map((group) => (
          <div key={group.day} className="mb-3">
            <p className="sticky top-0 bg-gray-950 py-1 text-[9px] font-semibold uppercase tracking-wide text-gray-500">
              {group.day}
            </p>
            <ul className="space-y-1">
              {group.items.map((e) => (
                <FeedRow key={e.id} entry={e} />
              ))}
            </ul>
          </div>
        ))}
      </div>
    </div>
  );
}

function FeedRow({ entry }: { entry: FeedEntry }) {
  const [open, setOpen] = useState(false);
  const hasPayload = Boolean(entry.payloadJson);

  return (
    <li className="rounded border border-gray-800 bg-gray-900/40 px-2 py-1.5 text-[11px]">
      <button
        type="button"
        onClick={() => hasPayload && setOpen((v) => !v)}
        className={`flex w-full items-start gap-2 text-left ${hasPayload ? "hover:bg-white/5" : "cursor-default"}`}
      >
        <span className="shrink-0 text-[10px] text-gray-600">{formatClock(entry.createdAt)}</span>
        <span
          className={`shrink-0 rounded border px-1 text-[9px] uppercase tracking-wide ${kindBadge(entry.kind)}`}
        >
          {entry.kind}
        </span>
        <span className="shrink-0 rounded bg-gray-800/60 px-1 text-[9px] uppercase tracking-wide text-gray-400">
          {entry.phase}
        </span>
        <span className="flex-1 truncate text-gray-200">{entry.summary}</span>
        {hasPayload && <span className="shrink-0 text-gray-600">{open ? "▾" : "▸"}</span>}
      </button>
      {open && entry.payloadJson && (
        <pre className="mt-1 max-h-40 overflow-auto rounded bg-black/30 p-1.5 text-[10px] text-gray-400">
          {safeJsonPretty(entry.payloadJson)}
        </pre>
      )}
    </li>
  );
}

function safeJsonPretty(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
