import type { ProjectSummary } from "./types";

export function projectKindLabel(project: ProjectSummary) {
  switch (project.kind) {
    case "workspace_candidate":
      return "Workspace";
    case "plugin_package":
      return "Plugin";
    case "git_repo":
    default:
      return "Git";
  }
}

export function projectSelectLabel(project: ProjectSummary) {
  const prefix = project.kind && project.kind !== "git_repo" ? `[${projectKindLabel(project)}] ` : "";
  return `${prefix}${project.name}`;
}
