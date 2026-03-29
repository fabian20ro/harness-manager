import { formatDisplayPath } from "../lib/inspect";
import type { JobStatus } from "../lib/types";

type ScanStatusBarProps = {
  job: JobStatus | null;
};

export function ScanStatusBar({ job }: ScanStatusBarProps) {
  if (!job) return null;

  const tone =
    job.status === "failed" ? "failed" : job.status === "completed" ? "completed" : "running";
  const counts =
    typeof job.items_done === "number" && typeof job.items_total === "number"
      ? `${job.items_done}/${job.items_total}`
      : null;

  return (
    <div
      className={`scan-status-bar ${tone}`}
      role="status"
      aria-live="polite"
      aria-label="Scan status"
    >
      <span className="scan-status-spinner" aria-hidden="true">
        {tone === "failed" ? "!" : tone === "completed" ? "✓" : "◌"}
      </span>
      <div className="scan-status-copy">
        <strong>{job.message}</strong>
        <span>
          {job.current_path ? formatDisplayPath(job.current_path) : "Waiting for scan progress."}
          {counts ? ` • ${counts}` : ""}
        </span>
      </div>
    </div>
  );
}
