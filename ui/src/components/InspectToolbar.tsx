import { HelperCommand } from "./HelperCommand";
import { LABELS, TOOL_IDS } from "../lib/inspect";
import { projectSelectLabel } from "../lib/projects";
import type { JobStatus, ProjectSummary } from "../lib/types";

type InspectToolbarProps = {
  apiBase: string;
  onApiBaseChange: (value: string) => void;
  onCopyHelper: () => void;
  onScopedReindex: () => void;
  projects: ProjectSummary[];
  scopedJob: JobStatus | null;
  selectedProject: string;
  selectedTool: string;
  onSelectProject: (projectId: string) => void;
  onSelectTool: (toolId: string) => void;
  scopedReindexDisabled: boolean;
};

export function InspectToolbar({
  apiBase,
  onApiBaseChange,
  onCopyHelper,
  onScopedReindex,
  projects,
  scopedJob,
  selectedProject,
  selectedTool,
  onSelectProject,
  onSelectTool,
  scopedReindexDisabled,
}: InspectToolbarProps) {
  const scopedStatus =
    scopedJob?.status === "running"
      ? scopedJob.current_path
        ? `${scopedJob.message} • ${scopedJob.current_path}`
        : scopedJob.message
      : scopedJob?.status && scopedJob.status !== "running"
        ? scopedJob.message
        : null;

  return (
    <header className="toolbar">
      <div className="toolbar-controls">
        <label>
          <span>Project</span>
          <select value={selectedProject} onChange={(event) => onSelectProject(event.target.value)}>
            <option value="">Select project</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {projectSelectLabel(project)}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>Tool</span>
          <select value={selectedTool} onChange={(event) => onSelectTool(event.target.value)}>
            {TOOL_IDS.map((tool) => (
              <option key={tool} value={tool}>
                {LABELS[tool]}
              </option>
            ))}
          </select>
        </label>
        <label className="api-field">
          <span>API Base</span>
          <input
            value={apiBase}
            onChange={(event) => onApiBaseChange(event.target.value)}
            placeholder="http://127.0.0.1:8765"
          />
        </label>
        <HelperCommand onCopy={onCopyHelper} />
        <div className="toolbar-action">
          <button
            className="toolbar-reindex"
            onClick={onScopedReindex}
            disabled={scopedReindexDisabled}
          >
            {scopedReindexDisabled ? "Reindexing..." : "Reindex current"}
          </button>
          {scopedStatus ? <span className="toolbar-action-status">{scopedStatus}</span> : null}
        </div>
      </div>
    </header>
  );
}
