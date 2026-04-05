import { calculateContextCost, formatDisplayPath } from "../lib/inspect";
import type { JobStatus, SurfaceState } from "../lib/types";

type ScanStatusBarProps = {
  job: JobStatus | null;
  message?: string;
  graph?: SurfaceState | null;
};

export function ScanStatusBar({ job, message, graph }: ScanStatusBarProps) {
  const { bytes, warning } = calculateContextCost(graph ?? null);
  const sizeKb = Math.round(bytes / 1024);

  if (!job && !message) {
    if (!graph || bytes === 0) return null;
    return (
      <div className={`status-notice ${warning ? 'failed' : 'info'}`} role="status" aria-live="polite" aria-label="Status">
        <span className="status-notice-icon" aria-hidden="true">
          {warning ? '!' : 'i'}
        </span>
        <div className="status-notice-copy">
          <span>Effective context size: {sizeKb} KB {warning && "(Approaching Gemini truncation limit)"}</span>
        </div>
      </div>
    );
  }

  if (!job) {
    return (
      <div className="status-notice info" role="status" aria-live="polite" aria-label="Status">
        <span className="status-notice-icon" aria-hidden="true">
          i
        </span>
        <div className="status-notice-copy">
          <span>{message}</span>
          {bytes > 0 && <span style={{marginLeft: 10, opacity: 0.8}}>| Context size: {sizeKb} KB {warning && '⚠️'}</span>}
        </div>
      </div>
    );
  }

  const tone =
    job.status === "failed" ? "failed" : job.status === "completed" ? "completed" : "running";
  const scopeLabel =
    job.scope_kind === "project_tool"
      ? `Project reindex${job.tool ? ` • ${job.tool}` : ""}`
      : "Global reindex";
  const counts =
    typeof job.items_done === "number" && typeof job.items_total === "number"
      ? `${job.items_done}/${job.items_total}`
      : null;

  return (
    <div className={`status-notice ${tone}`} role="status" aria-live="polite" aria-label="Status">
      <span className="status-notice-icon" aria-hidden="true">
        {tone === "failed" ? "!" : tone === "completed" ? "✓" : "◌"}
      </span>
      <div className="status-notice-copy">
        <strong>{scopeLabel}</strong>
        <span>
          {job.message}
          {job.current_path ? ` • ${formatDisplayPath(job.current_path)}` : ""}
          {counts ? ` • ${counts}` : ""}
          {bytes > 0 && ` | Context size: ${sizeKb} KB ${warning ? '⚠️' : ''}`}
        </span>
      </div>
    </div>
  );
}
