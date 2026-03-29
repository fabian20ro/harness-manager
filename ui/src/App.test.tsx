import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";

describe("App", () => {
  let eventSources: Array<{
    url: string;
    onmessage: ((event: MessageEvent<string>) => void) | null;
    onerror: (() => void) | null;
    close: ReturnType<typeof vi.fn>;
  }>;

  beforeEach(() => {
    vi.restoreAllMocks();
    eventSources = [];
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn(() => null),
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      configurable: true,
    });
    Object.defineProperty(window, "EventSource", {
      value: class MockEventSource {
        url: string;
        onmessage: ((event: MessageEvent<string>) => void) | null = null;
        onerror: (() => void) | null = null;
        close = vi.fn();

        constructor(url: string) {
          this.url = url;
          eventSources.push(this);
        }
      },
      configurable: true,
    });
  });

  it("renders helper copy control in the toolbar, not the sidebar", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue({
        ok: true,
        json: async () => [],
      } as Response);

    const { container } = render(<App />);

    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const copyButton = screen.getByRole("button", { name: "Copy" });
    const toolbar = container.querySelector(".toolbar");
    const nav = container.querySelector(".nav");

    expect(toolbar?.contains(copyButton)).toBe(true);
    expect(nav?.contains(copyButton)).toBe(false);
    expect(screen.getByText("cargo run")).toBeInTheDocument();
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

  it("renders inline scan status updates from the event stream", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue({
      ok: true,
      json: async () => [],
    } as Response);

    const { container } = render(<App />);

    await waitFor(() => expect(eventSources).toHaveLength(1));

    await act(async () => {
      eventSources[0].onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            id: "job-1",
            kind: "scan",
            status: "running",
            created_at: "2026-03-29T10:00:00Z",
            finished_at: null,
            message: "Scanning ~/git/demo",
            current_path: "~/git/demo",
            items_done: 1,
            items_total: 2,
          }),
        }),
      );
    });

    expect(screen.getByRole("status", { name: "Status" })).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Status" })).toHaveTextContent("Scanning ~/git/demo");
    expect(container.querySelector(".status-notice")).toBeInTheDocument();
    vi.useFakeTimers();

    await act(async () => {
      eventSources[0].onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            id: "job-1",
            kind: "scan",
            status: "completed",
            created_at: "2026-03-29T10:00:00Z",
            finished_at: "2026-03-29T10:00:03Z",
            message: "Indexed 1 project(s).",
          }),
        }),
      );
    });

    expect(screen.getByRole("status", { name: "Status" })).toHaveTextContent(
      "Indexed 1 project(s).",
    );
    await act(async () => {
      vi.advanceTimersByTime(4_001);
    });
    expect(screen.queryByRole("status", { name: "Status" })).not.toBeInTheDocument();
    vi.useRealTimers();
  });

  it("shows scoped reindex busy state only for the active project and tool", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const url = String(input);
      if (url.endsWith("/api/projects")) {
        return {
          ok: true,
          json: async () => [
            {
              id: "p1",
              root_path: "/tmp/demo",
              display_path: "~/git/demo",
              name: "demo",
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
          ],
        } as Response;
      }
      if (url.includes("/api/projects/p1/graph?tool=codex")) {
        return {
          ok: true,
          json: async () => ({
            project: {
              id: "p1",
              root_path: "/tmp/demo",
              display_path: "~/git/demo",
              name: "demo",
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
            tool: {
              id: "codex",
              display_name: "Codex",
              support_level: "full",
            },
            nodes: [
              { id: "tool:codex", kind: "tool_context" },
              {
                id: "repo-file",
                kind: "artifact",
                path: "/tmp/demo/AGENTS.md",
                display_path: "~/git/demo/AGENTS.md",
              },
            ],
            edges: [],
            verdicts: [{ entity_id: "repo-file", states: ["effective"], why_included: [], why_excluded: [] }],
          }),
        } as Response;
      }
      if (url.includes("/api/projects/p1/inspect?tool=codex")) {
        return {
          ok: true,
          json: async () => ({
            entity: {
              id: "repo-file",
              kind: "artifact",
              path: "/tmp/demo/AGENTS.md",
              display_path: "~/git/demo/AGENTS.md",
            },
            verdict: { states: ["effective"], why_included: [], why_excluded: [], shadowed_by: [] },
            incoming_edges: [],
            outgoing_edges: [],
            related_activity: [],
            viewer_content: "alpha",
            edit: { editable: false, last_saved_backup_available: false },
          }),
        } as Response;
      }
      return {
        ok: true,
        json: async () => [],
      } as Response;
    });

    render(<App />);

    await waitFor(() => expect(eventSources).toHaveLength(1));
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Project" })).toHaveValue("p1"),
    );

    await act(async () => {
      eventSources[0].onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            id: "job-2",
            kind: "scan",
            scope_kind: "project_tool",
            project_id: "p1",
            tool: "codex",
            status: "running",
            created_at: "2026-03-29T10:00:00Z",
            finished_at: null,
            message: "Evaluating Codex for ~/git/demo",
          }),
        }),
      );
    });

    expect(screen.getByRole("button", { name: "Reindexing..." })).toBeDisabled();
  });

  it("starts with the tree expanded and keeps selection after collapsing an ancestor", async () => {
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn((key: string) => {
          if (key === "harnessInspector.activeTab") {
            return JSON.stringify("Inspect");
          }
          if (key === "harnessInspector.selectedTool") {
            return "codex";
          }
          return null;
        }),
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      configurable: true,
    });

    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const url = String(input);
      if (url.endsWith("/api/projects")) {
        return {
          ok: true,
          json: async () => [
            {
              id: "p1",
              root_path: "/tmp/demo",
              display_path: "~/git/demo",
              name: "demo",
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
          ],
        } as Response;
      }
      if (url.includes("/api/projects/p1/graph?tool=codex")) {
        return {
          ok: true,
          json: async () => ({
            project: {
              id: "p1",
              root_path: "/tmp/demo",
              display_path: "~/git/demo",
              name: "demo",
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
            tool: {
              id: "codex",
              display_name: "Codex",
              support_level: "full",
            },
            nodes: [
              { id: "tool:codex", kind: "tool_context" },
              {
                id: "repo-file",
                kind: "artifact",
                path: "/tmp/demo/docs/AGENTS.md",
                display_path: "~/git/demo/docs/AGENTS.md",
              },
              {
                id: "policy-file",
                kind: "artifact",
                path: "/tmp/demo/notes/policy.md",
                display_path: "~/git/demo/notes/policy.md",
              },
            ],
            edges: [],
            verdicts: [
              { entity_id: "repo-file", states: ["effective"], why_included: [], why_excluded: [] },
              {
                entity_id: "policy-file",
                states: ["referenced_only"],
                why_included: [],
                why_excluded: [],
              },
            ],
          }),
        } as Response;
      }
      if (url.includes("/api/projects/p1/inspect?tool=codex")) {
        return {
          ok: true,
          json: async () => ({
            entity: {
              id: "repo-file",
              kind: "artifact",
              path: "/tmp/demo/docs/AGENTS.md",
              display_path: "~/git/demo/docs/AGENTS.md",
            },
            verdict: { states: ["effective"], why_included: [], why_excluded: [], shadowed_by: [] },
            incoming_edges: [],
            outgoing_edges: [],
            related_activity: [],
            viewer_content: "alpha",
            edit: { editable: false, last_saved_backup_available: false },
          }),
        } as Response;
      }
      return {
        ok: true,
        json: async () => [],
      } as Response;
    });

    render(<App />);
    await act(async () => {
      screen.getByRole("button", { name: "Inspect" }).click();
    });

    await waitFor(() => expect(screen.getByRole("button", { name: "Collapse git" })).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Select AGENTS.md" })).toBeInTheDocument();

    await act(async () => {
      screen.getByRole("button", { name: "Collapse git" }).click();
    });

    expect(screen.queryByRole("button", { name: "Select AGENTS.md" })).not.toBeInTheDocument();
    expect(screen.getByText("alpha")).toBeInTheDocument();
  });

  it("uses stored tree expansion state instead of the default fully-expanded seed", async () => {
    const storedTreeKey = "harnessInspector.inspectTreeExpanded.p1.codex";
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn((key: string) => {
          if (key === "harnessInspector.activeTab") {
            return JSON.stringify("Inspect");
          }
          if (key === "harnessInspector.selectedTool") {
            return "codex";
          }
          if (key === storedTreeKey) {
            return JSON.stringify(["~"]);
          }
          return null;
        }),
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      configurable: true,
    });

    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const url = String(input);
      if (url.endsWith("/api/projects")) {
        return {
          ok: true,
          json: async () => [
            {
              id: "p1",
              root_path: "/tmp/demo",
              display_path: "~/git/demo",
              name: "demo",
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
          ],
        } as Response;
      }
      if (url.includes("/api/projects/p1/graph?tool=codex")) {
        return {
          ok: true,
          json: async () => ({
            project: {
              id: "p1",
              root_path: "/tmp/demo",
              display_path: "~/git/demo",
              name: "demo",
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
            tool: {
              id: "codex",
              display_name: "Codex",
              support_level: "full",
            },
            nodes: [
              { id: "tool:codex", kind: "tool_context" },
              {
                id: "repo-file",
                kind: "artifact",
                path: "/tmp/demo/docs/AGENTS.md",
                display_path: "~/git/demo/docs/AGENTS.md",
              },
              {
                id: "policy-file",
                kind: "artifact",
                path: "/tmp/demo/notes/policy.md",
                display_path: "~/git/demo/notes/policy.md",
              },
            ],
            edges: [],
            verdicts: [
              { entity_id: "repo-file", states: ["effective"], why_included: [], why_excluded: [] },
              {
                entity_id: "policy-file",
                states: ["referenced_only"],
                why_included: [],
                why_excluded: [],
              },
            ],
          }),
        } as Response;
      }
      if (url.includes("/api/projects/p1/inspect?tool=codex")) {
        return {
          ok: true,
          json: async () => ({
            entity: {
              id: "repo-file",
              kind: "artifact",
              path: "/tmp/demo/docs/AGENTS.md",
              display_path: "~/git/demo/docs/AGENTS.md",
            },
            verdict: { states: ["effective"], why_included: [], why_excluded: [], shadowed_by: [] },
            incoming_edges: [],
            outgoing_edges: [],
            related_activity: [],
            viewer_content: "alpha",
            edit: { editable: false, last_saved_backup_available: false },
          }),
        } as Response;
      }
      return {
        ok: true,
        json: async () => [],
      } as Response;
    });

    render(<App />);
    await act(async () => {
      screen.getByRole("button", { name: "Inspect" }).click();
    });

    await waitFor(() => expect(screen.getByRole("button", { name: "Collapse git" })).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Expand notes" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Select policy.md" })).not.toBeInTheDocument();
  });

  it("supports expand all and collapse all tree actions", async () => {
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn((key: string) => {
          if (key === "harnessInspector.activeTab") {
            return JSON.stringify("Inspect");
          }
          if (key === "harnessInspector.selectedTool") {
            return "codex";
          }
          return null;
        }),
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      configurable: true,
    });

    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const url = String(input);
      if (url.endsWith("/api/projects")) {
        return {
          ok: true,
          json: async () => [
            {
              id: "p1",
              root_path: "/tmp/demo",
              display_path: "~/git/demo",
              name: "demo",
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
          ],
        } as Response;
      }
      if (url.includes("/api/projects/p1/graph?tool=codex")) {
        return {
          ok: true,
          json: async () => ({
            project: {
              id: "p1",
              root_path: "/tmp/demo",
              display_path: "~/git/demo",
              name: "demo",
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
            tool: {
              id: "codex",
              display_name: "Codex",
              support_level: "full",
            },
            nodes: [
              { id: "tool:codex", kind: "tool_context" },
              {
                id: "root-file",
                kind: "artifact",
                path: "/tmp/demo/docs/root.md",
                display_path: "~/git/demo/docs/root.md",
              },
              {
                id: "deep-file",
                kind: "artifact",
                path: "/tmp/demo/docs/nested/deep.md",
                display_path: "~/git/demo/docs/nested/deep.md",
              },
            ],
            edges: [],
            verdicts: [
              { entity_id: "root-file", states: ["effective"], why_included: [], why_excluded: [] },
              { entity_id: "deep-file", states: ["effective"], why_included: [], why_excluded: [] },
            ],
          }),
        } as Response;
      }
      if (url.includes("/api/projects/p1/inspect?tool=codex")) {
        return {
          ok: true,
          json: async () => ({
            entity: {
              id: "root-file",
              kind: "artifact",
              path: "/tmp/demo/docs/root.md",
              display_path: "~/git/demo/docs/root.md",
            },
            verdict: { states: ["effective"], why_included: [], why_excluded: [], shadowed_by: [] },
            incoming_edges: [],
            outgoing_edges: [],
            related_activity: [],
            viewer_content: "alpha",
            edit: { editable: false, last_saved_backup_available: false },
          }),
        } as Response;
      }
      return {
        ok: true,
        json: async () => [],
      } as Response;
    });

    render(<App />);

    await waitFor(() => expect(screen.getByRole("button", { name: "Select deep.md" })).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Select deep.md" })).toBeInTheDocument();

    await act(async () => {
      screen.getByRole("button", { name: "Collapse all" }).click();
    });
    expect(screen.queryByRole("button", { name: "Select deep.md" })).not.toBeInTheDocument();

    await act(async () => {
      screen.getByRole("button", { name: "Expand all" }).click();
    });
    expect(screen.getByRole("button", { name: "Select deep.md" })).toBeInTheDocument();
  });

  it("clears stale inspect errors after a successful inspect fetch", async () => {
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn((key: string) => {
          if (key === "harnessInspector.activeTab") {
            return JSON.stringify("Inspect");
          }
          if (key === "harnessInspector.selectedTool") {
            return "codex";
          }
          return null;
        }),
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      configurable: true,
    });

    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const url = String(input);
      if (url.endsWith("/api/projects")) {
        return {
          ok: true,
          json: async () => [
            {
              id: "p1",
              root_path: "/tmp/demo",
              display_path: "~/git/demo",
              name: "demo",
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
          ],
        } as Response;
      }
      if (url.includes("/api/projects/p1/graph?tool=codex")) {
        return {
          ok: true,
          json: async () => ({
            project: {
              id: "p1",
              root_path: "/tmp/demo",
              display_path: "~/git/demo",
              name: "demo",
              indexed_at: "2026-03-29T10:00:00Z",
              status: "ready",
            },
            tool: {
              id: "codex",
              display_name: "Codex",
              support_level: "full",
            },
            nodes: [
              { id: "tool:codex", kind: "tool_context" },
              {
                id: "missing-file",
                kind: "artifact",
                path: "/tmp/demo/docs/missing.md",
                display_path: "~/git/demo/docs/missing.md",
              },
              {
                id: "good-file",
                kind: "artifact",
                path: "/tmp/demo/docs/policy.md",
                display_path: "~/git/demo/docs/policy.md",
              },
            ],
            edges: [],
            verdicts: [
              { entity_id: "missing-file", states: ["effective"], why_included: [], why_excluded: [] },
              {
                entity_id: "good-file",
                states: ["referenced_only"],
                why_included: [],
                why_excluded: [],
              },
            ],
          }),
        } as Response;
      }
      if (url.includes("node=missing-file")) {
        return {
          ok: false,
          status: 404,
          json: async () => ({ error: "node not found" }),
        } as Response;
      }
      if (url.includes("node=good-file")) {
        return {
          ok: true,
          json: async () => ({
            entity: {
              id: "good-file",
              kind: "artifact",
              path: "/tmp/demo/docs/policy.md",
              display_path: "~/git/demo/docs/policy.md",
            },
            verdict: { states: ["effective"], why_included: [], why_excluded: [], shadowed_by: [] },
            incoming_edges: [],
            outgoing_edges: [],
            related_activity: [],
            viewer_content: "beta",
            edit: { editable: false, last_saved_backup_available: false },
          }),
        } as Response;
      }
      return {
        ok: true,
        json: async () => [],
      } as Response;
    });

    render(<App />);

    await waitFor(() =>
      expect(screen.getByText(/Inspect failed for ~\/git\/demo\/docs\/missing\.md/)).toBeInTheDocument(),
    );

    await act(async () => {
      screen.getByRole("button", { name: "Select policy.md" }).click();
    });

    await waitFor(() => expect(screen.getByText("beta")).toBeInTheDocument());
    expect(
      screen.queryByText(/Inspect failed for ~\/git\/demo\/docs\/missing\.md/),
    ).not.toBeInTheDocument();
  });
});
