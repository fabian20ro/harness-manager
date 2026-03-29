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

  it("renders bottom scan status bar from event stream updates", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue({
      ok: true,
      json: async () => [],
    } as Response);

    render(<App />);

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

    expect(screen.getByRole("status", { name: "Scan status" })).toBeInTheDocument();
    expect(screen.getByText("Scanning ~/git/demo")).toBeInTheDocument();
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

    expect(screen.getByRole("status", { name: "Scan status" })).toHaveTextContent(
      "Indexed 1 project(s).",
    );
    await act(async () => {
      vi.advanceTimersByTime(4_001);
    });
    expect(screen.queryByRole("status", { name: "Scan status" })).not.toBeInTheDocument();
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
});
