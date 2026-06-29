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

  it("renders all capability sections when appropriate nodes are present in the graph", () => {
    const fullGraph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Skill 1" },
        { id: "hook-1", kind: "node", artifact_type: "hook", name: "Hook 1" },
        { id: "mcp-1", kind: "node", artifact_type: "mcp", name: "MCP 1" },
        { id: "instr-1", kind: "node", artifact_type: "instructions", name: "Instr 1" },
      ],
      edges: [],
      verdicts: [
        { entity_id: "skill-1", states: ["effective"], why_included: [], why_excluded: [] },
        { entity_id: "hook-1", states: ["effective"], why_included: [], why_excluded: [] },
        { entity_id: "mcp-1", states: ["effective"], why_included: [], why_excluded: [] },
        { entity_id: "instr-1", states: ["effective"], why_included: [], why_excluded: [] },
      ],
    };

    render(<CapabilitiesDashboard graph={fullGraph} />);

    expect(screen.getByText(/Skills/i)).toBeInTheDocument();
    expect(screen.getByText(/Hooks & Scripts/i)).toBeInTheDocument();
    expect(screen.getByText(/MCP Servers/i)).toBeInTheDocument();
    expect(screen.getByText(/Instructions & Agents/i)).toBeInTheDocument();
  });
});
