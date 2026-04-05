import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";
import { setupTestMocks } from "./testMocks";

describe("Projects View", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setupTestMocks();
  });

  it("renders project tiers distinctly in the projects list", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const url = String(input);
      if (url.endsWith("/api/projects")) {
        return {
          ok: true,
          json: async () => [
            {
              id: "git-1",
              root_path: "/tmp/repo",
              display_path: "~/git/repo",
              name: "repo",
              kind: "git_repo",
              discovery_reason: "Directory contains .git.",
              signal_score: 300,
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
            {
              id: "ws-1",
              root_path: "/tmp/workspace",
              display_path: "~/scratch/workspace",
              name: "workspace",
              kind: "workspace_candidate",
              discovery_reason: "Workspace contains AGENTS.md.",
              signal_score: 220,
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
            {
              id: "plugin-1",
              root_path: "/tmp/plugin",
              display_path: "~/.codex/.tmp/plugins/plugins/vercel",
              name: "vercel",
              kind: "plugin_package",
              discovery_reason: "Plugin package contains .mcp.json.",
              signal_score: 120,
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
          ],
        } as Response;
      }
      if (url.includes("/graph?tool=")) {
        return {
          ok: true,
          json: async () => ({
            project: {
              id: "git-1",
              root_path: "/tmp/repo",
              display_path: "~/git/repo",
              name: "repo",
              kind: "git_repo",
              discovery_reason: "Directory contains .git.",
              signal_score: 300,
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
            tool: { id: "codex", display_name: "Codex", support_level: "full" },
            nodes: [{ id: "tool:codex", kind: "tool_context" }],
            edges: [],
            verdicts: [],
          }),
        } as Response;
      }
      return { ok: true, json: async () => ({}) } as Response;
    });

    render(<App />);

    expect(await screen.findByText("Directory contains .git.")).toBeInTheDocument();
    expect(screen.getByText("Workspace contains AGENTS.md.")).toBeInTheDocument();
    expect(screen.getByText("Plugin package contains .mcp.json.")).toBeInTheDocument();
    expect(screen.getByText("Git")).toBeInTheDocument();
    expect(screen.getByText("Workspace")).toBeInTheDocument();
    expect(screen.getByText("Plugin")).toBeInTheDocument();
  });
});
