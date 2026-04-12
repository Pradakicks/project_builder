import { useLeaderStore } from "../../store/useLeaderStore";
import { Markdown } from "../ui/Markdown";

const statusColors: Record<string, string> = {
  merging: "bg-yellow-600",
  merged: "bg-green-600",
  conflict: "bg-red-600",
  "conflict-resolving": "bg-yellow-600",
  "conflict-resolved": "bg-green-600",
  failed: "bg-red-700",
  skipped: "bg-gray-600",
};

export function MergeSection({ planId }: { planId: string }) {
  const {
    merging,
    mergeStatus,
    mergeError,
    mergeProgress,
    mergeSummary,
    conflictInfo,
    resolvingConflict,
    reviewStreaming,
    reviewStatus,
    reviewError,
    reviewOutput,
    mergeBranches,
    resolveConflict,
    runReview,
  } = useLeaderStore();

  const hasReview =
    reviewOutput || reviewStreaming || reviewError || reviewStatus === "complete";
  const mergeActionLabel = mergeError && !conflictInfo ? "Retry merge" : "Merge All";
  const reviewActionLabel = reviewError ? "Retry review" : "Run review";
  const lifecycleLabel =
    merging
      ? "Merging"
      : mergeStatus === "conflict"
        ? "Merge paused"
        : mergeStatus === "failed"
          ? "Merge failed"
          : reviewStreaming
            ? "Reviewing"
            : reviewStatus === "failed"
              ? "Review failed"
              : reviewStatus === "complete"
                ? "Review complete"
                : mergeSummary
                  ? "Merged"
                  : "Ready";

  return (
    <div className="space-y-2 border-t border-gray-700 pt-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
          Merge & Review
        </p>
        <div className="flex items-center gap-2">
          <span
            className={`rounded px-1.5 py-0.5 text-[9px] font-medium text-white ${
              lifecycleLabel === "Merged" || lifecycleLabel === "Review complete"
                ? "bg-green-600"
                : lifecycleLabel === "Reviewing" || lifecycleLabel === "Merging"
                  ? "bg-blue-600"
                  : lifecycleLabel === "Merge paused" || lifecycleLabel === "Merge failed" || lifecycleLabel === "Review failed"
                    ? "bg-red-600"
                    : "bg-gray-600"
            }`}
          >
            {lifecycleLabel}
          </span>
          {!merging && !mergeSummary && (
            <button
              onClick={() => mergeBranches(planId)}
              className="rounded bg-emerald-600 px-2.5 py-1 text-[10px] font-medium text-white hover:bg-emerald-500 transition-colors"
            >
              {mergeActionLabel}
            </button>
          )}
          {merging && (
            <span className="text-[10px] text-emerald-300 animate-pulse">
              Merging...
            </span>
          )}
        </div>
      </div>

      {/* Branch status cards */}
      {mergeProgress.length > 0 && (
        <div className="space-y-1">
          {mergeProgress.map((p) => (
            <div
              key={p.branch}
              className="flex items-center gap-2 rounded border border-gray-700 bg-gray-800/50 px-2 py-1.5"
            >
              <span
                className={`shrink-0 rounded px-1.5 py-0.5 text-[9px] font-medium text-white ${statusColors[p.status] || "bg-gray-600"}`}
              >
                {p.status}
              </span>
              <span className="text-[10px] text-gray-300 truncate flex-1">
                {p.pieceName}
              </span>
              <span className="text-[9px] text-gray-500 font-mono truncate">
                {p.branch}
              </span>
            </div>
          ))}
        </div>
      )}

      {/* Merge summary stats */}
      {mergeSummary && !conflictInfo && (
        <div className="rounded border border-gray-700 bg-gray-800/50 p-2 space-y-1">
          <div className="flex gap-3 text-[10px]">
            {mergeSummary.merged.length > 0 && (
              <span className="text-green-400">
                {mergeSummary.merged.length} merged
              </span>
            )}
            {mergeSummary.skipped.length > 0 && (
              <span className="text-gray-500">
                {mergeSummary.skipped.length} skipped
              </span>
            )}
          </div>
          {mergeSummary.combinedDiffStat && (
            <pre className="text-[9px] text-gray-500 whitespace-pre-wrap">
              {mergeSummary.combinedDiffStat}
            </pre>
          )}
        </div>
      )}

      {mergeError && !conflictInfo && (
        <div className="rounded border border-red-800 bg-red-900/20 p-2 space-y-1">
          <p className="text-[10px] font-semibold text-red-300">
            Merge failure
          </p>
          <p className="text-[10px] text-gray-300 leading-relaxed">
            {mergeError}
          </p>
        </div>
      )}

      {/* Conflict panel */}
      {conflictInfo && (
        <div className="rounded border border-red-800 bg-red-900/20 p-2 space-y-2">
          <div className="flex items-center gap-2">
            <span className="text-[10px] font-semibold text-red-400">
              Merge Conflict
            </span>
            <span className="text-[10px] text-gray-400">
              {conflictInfo.pieceName} ({conflictInfo.conflictingFiles.length} file
              {conflictInfo.conflictingFiles.length !== 1 ? "s" : ""})
            </span>
          </div>
          <div className="text-[9px] text-gray-500 space-y-0.5">
            {conflictInfo.conflictingFiles.map((f) => (
              <div key={f} className="font-mono">{f}</div>
            ))}
          </div>
          {conflictInfo.conflictDiff && (
            <details className="text-[9px]">
              <summary className="text-gray-500 cursor-pointer hover:text-gray-300">
                Show diff
              </summary>
              <pre className="mt-1 max-h-48 overflow-auto rounded bg-gray-900 p-2 text-gray-400 whitespace-pre-wrap font-mono">
                {conflictInfo.conflictDiff.slice(0, 5000)}
              </pre>
            </details>
          )}
          <div className="flex gap-2">
            <button
              onClick={() => resolveConflict(planId, conflictInfo.pieceId)}
              disabled={resolvingConflict}
              className="rounded bg-blue-600 px-2.5 py-1 text-[10px] font-medium text-white hover:bg-blue-500 disabled:opacity-50 transition-colors"
            >
              {resolvingConflict ? "Resolving..." : "Resolve with AI"}
            </button>
          </div>
        </div>
      )}

      {/* Integration review */}
      {hasReview && (
        <div className="space-y-1">
          <p className="text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
            Integration Review
          </p>
          <div className="rounded border border-gray-700 bg-gray-800/50 p-2">
            {reviewStreaming && !reviewOutput && (
              <span className="text-[10px] text-gray-500 animate-pulse">
                Reviewing integration...
              </span>
            )}
            {reviewError && !reviewStreaming && (
              <div className="space-y-1">
                <p className="text-[10px] font-semibold text-red-300">
                  Review failure
                </p>
                <p className="text-[10px] text-gray-300 leading-relaxed">
                  {reviewError}
                </p>
              </div>
            )}
            {reviewOutput && <Markdown content={reviewOutput} />}
            {!reviewStreaming && !reviewOutput && !reviewError && (
              <p className="text-[10px] text-gray-500">
                {reviewStatus === "complete"
                  ? "Integration review completed without markdown output."
                  : "Review output will appear here after the merge completes."}
              </p>
            )}
          </div>
          {!reviewStreaming && (reviewOutput || reviewError) && (
            <button
              onClick={() => runReview(planId)}
              className="text-[9px] text-gray-500 hover:text-gray-300"
            >
              {reviewActionLabel}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
