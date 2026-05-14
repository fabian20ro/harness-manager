import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";
import { setupTestMocks } from "./testMocks";

describe("App Navigation", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setupTestMocks();
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

  it("marks app shell action buttons as button type", async () => {
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
      if (url.includes("/api/projects/p1/inspect?tool=codex")) {
        return {
          ok: true,
          json: async () => ({
            entity: {
              id: "tool:codex",
              kind: "tool_context",
              path: "~/git/demo",
              display_path: "~/git/demo",
            },
            verdict: { states: [], why_included: [], why_excluded: [], shadowed_by: [] },
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

    await waitFor(() => expect(screen.getByRole("button", { name: /Reindex all/i })).toHaveAttribute("type", "button"));
    await waitFor(() => expect(screen.getByRole("button", { name: /demo/i })).toHaveAttribute("type", "button"));

    fireEvent.click(screen.getByRole("button", { name: "Docs" }));
    expect(screen.getByRole("button", { name: "Fetch snapshot" })).toHaveAttribute("type", "button");

    fireEvent.click(screen.getByRole("button", { name: "Activity" }));
    expect(screen.getByRole("button", { name: "Refresh" })).toHaveAttribute("type", "button");

    fireEvent.click(screen.getByRole("button", { name: "Inspect" }));
    expect(screen.getByRole("button", { name: "Expand" })).toHaveAttribute("type", "button");
    expect(screen.getByRole("button", { name: "Collapse" })).toHaveAttribute("type", "button");
  });
});
