import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { CapabilitiesDashboard } from "./CapabilitiesDashboard";
import type { SurfaceState } from "../lib/types";

const baseGraph: SurfaceState = {
  project: {
    id: "project-1",
    root_path: "/workspace/project",
    display_path: "project",
    name: "project",
    indexed_at: "2026-05-13T08:00:00Z",
    status: "ready",
  },
  tool: {
    id: "claude",
    display_name: "Claude Code",
    support_level: "supported",
  },
  nodes: [],
  edges: [],
  verdicts: [],
};

describe("CapabilitiesDashboard", () => {
  it("shows an actionable empty state when no graph is selected", () => {
    render(<CapabilitiesDashboard graph={null} />);

    expect(
      screen.getByText(
        "Select a project and tool context to see skills, hooks, MCP servers, and instructions.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveAttribute("aria-live", "polite");
  });

  it("shows an empty discovered-capabilities state when the graph has no capability nodes", () => {
    render(<CapabilitiesDashboard graph={baseGraph} />);

    expect(
      screen.getByText(
        "No skills, hooks, MCP servers, or instructions were discovered for this project and tool context yet.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveAttribute("aria-atomic", "true");
  });
});
