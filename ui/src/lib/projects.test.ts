import { describe, expect, it } from "vitest";
import { projectKindLabel, projectSelectLabel } from "./projects";
import type { ProjectSummary } from "./types";

const mockProject = (overrides: Partial<ProjectSummary>): ProjectSummary => ({
  id: "test-id",
  root_path: "/test/path",
  display_path: "~/test/path",
  name: "test-project",
  indexed_at: new Date().toISOString(),
  status: "ready",
  ...overrides,
});

describe("projects helpers", () => {
  describe("projectKindLabel", () => {
    it('returns "Workspace" for workspace_candidate', () => {
      const project = mockProject({ kind: "workspace_candidate" });
      expect(projectKindLabel(project)).toBe("Workspace");
    });

    it('returns "Plugin" for plugin_package', () => {
      const project = mockProject({ kind: "plugin_package" });
      expect(projectKindLabel(project)).toBe("Plugin");
    });

    it('returns "Git" for git_repo', () => {
      const project = mockProject({ kind: "git_repo" });
      expect(projectKindLabel(project)).toBe("Git");
    });

    it('returns "Git" for undefined kind', () => {
      const project = mockProject({ kind: undefined });
      expect(projectKindLabel(project)).toBe("Git");
    });
  });

  describe("projectSelectLabel", () => {
    it("includes [Workspace] prefix for workspace candidates", () => {
      const project = mockProject({ name: "my-app", kind: "workspace_candidate" });
      expect(projectSelectLabel(project)).toBe("[Workspace] my-app");
    });

    it("includes [Plugin] prefix for plugin packages", () => {
      const project = mockProject({ name: "my-plugin", kind: "plugin_package" });
      expect(projectSelectLabel(project)).toBe("[Plugin] my-plugin");
    });

    it("omits prefix for git repos", () => {
      const project = mockProject({ name: "my-repo", kind: "git_repo" });
      expect(projectSelectLabel(project)).toBe("my-repo");
    });

    it("omits prefix when kind is missing", () => {
      const project = mockProject({ name: "my-repo", kind: undefined });
      expect(projectSelectLabel(project)).toBe("my-repo");
    });
  });
});
