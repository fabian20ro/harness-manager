import { formatDisplayPath } from "../lib/inspect";
import type { JobStatus } from "../lib/types";

type ScanStatusBarProps = {
  job: JobStatus | null;
  message?: string;
};

export function ScanStatusBar({ job, message }: ScanStatusBarProps) {
  if (!job && !message) return null;

  if (!job) {
    return (
      <div className="status-notice info" role="status" aria-live="polite" aria-label="Status">
        <span className="status-notice-icon" aria-hidden="true">
          i
        </span>
        <div className="status-notice-copy">
          <span>{message}</span>
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
        </span>
      </div>
    </div>
  );
}
