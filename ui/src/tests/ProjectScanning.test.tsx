import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";
import { setupTestMocks } from "./testMocks";

describe("Project Scanning", () => {
  let eventSources: any[];

  beforeEach(() => {
    vi.restoreAllMocks();
    const mocks = setupTestMocks();
    eventSources = mocks.eventSources;
  });

  afterEach(() => {
    vi.useRealTimers();
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

  it("polls job status after starting scoped reindex and refreshes graph on completion", async () => {
    let graphCalls = 0;
    let jobCalls = 0;

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
        graphCalls += 1;
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
            nodes: [{ id: "tool:codex", kind: "tool_context" }],
            edges: [],
            verdicts: [],
          }),
        } as Response;
      }
      if (url.includes("/api/projects/p1/reindex")) {
        return {
          ok: true,
          json: async () => ({
            id: "job-9",
            kind: "scan",
            scope_kind: "project_tool",
            project_id: "p1",
            tool: "codex",
            status: "running",
            created_at: "2026-03-29T10:00:00Z",
            finished_at: null,
            message: "Reindexing codex.",
          }),
        } as Response;
      }
      if (url.includes("/api/jobs/job-9")) {
        jobCalls += 1;
        return {
          ok: true,
          json: async () =>
            jobCalls >= 2
              ? {
                  id: "job-9",
                  kind: "scan",
                  scope_kind: "project_tool",
                  project_id: "p1",
                  tool: "codex",
                  status: "completed",
                  created_at: "2026-03-29T10:00:00Z",
                  finished_at: "2026-03-29T10:00:02Z",
                  message: "Reindexed Codex for ~/git/demo.",
                }
              : {
                  id: "job-9",
                  kind: "scan",
                  scope_kind: "project_tool",
                  project_id: "p1",
                  tool: "codex",
                  status: "running",
                  created_at: "2026-03-29T10:00:00Z",
                  finished_at: null,
                  message: "Scanning ~/git/demo",
                },
        } as Response;
      }
      return {
        ok: true,
        json: async () => ({
          entity: { id: "tool:codex", kind: "tool_context" },
          verdict: { states: [], why_included: [], why_excluded: [], shadowed_by: [] },
          incoming_edges: [],
          outgoing_edges: [],
          related_activity: [],
          viewer_content: "",
          edit: { editable: false, last_saved_backup_available: false },
        }),
      } as Response;
    });

    render(<App />);

    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Project" })).toHaveValue("p1"),
    );
    expect(graphCalls).toBe(1);

    await act(async () => {
      screen.getByRole("button", { name: "Reindex current" }).click();
    });

    expect(screen.getByRole("button", { name: "Reindexing..." })).toBeDisabled();

    await waitFor(() => expect(jobCalls).toBeGreaterThanOrEqual(2), { timeout: 3000 });
    await waitFor(() => expect(graphCalls).toBe(2), { timeout: 3000 });
  });

  it("shows conflict text when a scan job is already running", async () => {
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
            nodes: [{ id: "tool:codex", kind: "tool_context" }],
            edges: [],
            verdicts: [],
          }),
        } as Response;
      }
      if (url.includes("/api/projects/p1/reindex")) {
        return {
          ok: false,
          status: 409,
          json: async () => ({
            error: "Another scan or reindex job is already running.",
          }),
        } as Response;
      }
      return {
        ok: true,
        json: async () => ({
          entity: { id: "tool:codex", kind: "tool_context" },
          verdict: { states: [], why_included: [], why_excluded: [], shadowed_by: [] },
          incoming_edges: [],
          outgoing_edges: [],
          related_activity: [],
          viewer_content: "",
          edit: { editable: false, last_saved_backup_available: false },
        }),
      } as Response;
    });

    render(<App />);

    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Project" })).toHaveValue("p1"),
    );

    await act(async () => {
      screen.getByRole("button", { name: "Reindex current" }).click();
    });

    await waitFor(() =>
      expect(
        screen.getByText("Error: Another scan or reindex job is already running."),
      ).toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: "Reindex current" })).toBeEnabled();
  });
});
