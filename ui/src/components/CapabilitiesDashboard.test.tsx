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

  it("renders correct status badges based on node usage states", () => {
    const graphWithUsage: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-used", kind: "node", artifact_type: "skill", name: "Used Skill" },
        { id: "skill-broken", kind: "node", artifact_type: "skill", name: "Broken Skill" },
        { id: "skill-proposed", kind: "node", artifact_type: "skill", name: "Proposed Skill" },
        { id: "skill-unused", kind: "node", artifact_type: "skill", name: "Unused Skill" },
      ],
      edges: [],
      verdicts: [
        { entity_id: "skill-used", states: ["effective"], why_included: [], why_excluded: [] },
        { entity_id: "skill-broken", states: ["unresolved"], why_included: [], why_excluded: [] },
        { entity_id: "skill-proposed", states: ["proposed"], why_included: [], why_excluded: [] },
        { entity_id: "skill-unused", states: [], why_included: [], why_excluded: [] },
      ],
    };

    render(<CapabilitiesDashboard graph={graphWithUsage} />);

    expect(screen.getByText(/Effective/i, { selector: 'span' })).toBeInTheDocument();
    expect(screen.getByText(/Broken/i, { selector: 'span' })).toBeInTheDocument();
    expect(screen.getByText(/Proposed/i, { selector: 'span' })).toBeInTheDocument();
    expect(screen.getByText(/Inactive/i, { selector: 'span' })).toBeInTheDocument();
  });

  it("correctly categorizes 'script' as Hooks & Scripts and 'agent' as Instructions & Agents", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "script-1", kind: "node", artifact_type: "script", name: "Script 1" },
        { id: "agent-1", kind: "node", artifact_type: "agent", name: "Agent 1" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText(/Hooks & Scripts/i)).toBeInTheDocument();
    expect(screen.getByText(/Instructions & Agents/i)).toBeInTheDocument();
    expect(screen.queryByText(/Skills/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/MCP Servers/i)).not.toBeInTheDocument();
  });

  it("renders the correct count for each section", () => {
    const graphWithCounts: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Skill 1" },
        { id: "skill-2", kind: "node", artifact_type: "skill", name: "Skill 2" },
        { id: "hook-1", kind: "node", artifact_type: "hook", name: "Hook 1" },
        { id: "mcp-1", kind: "node", artifact_type: "mcp", name: "MCP 1" },
        { id: "instr-1", kind: "node", artifact_type: "instructions", name: "Instr 1" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graphWithCounts} />);

    expect(screen.getByText(/Skills/i)).toHaveTextContent("(2)");
    expect(screen.getByText(/Hooks & Scripts/i)).toHaveTextContent("(1)");
    expect(screen.getByText(/MCP Servers/i)).toHaveTextContent("(1)");
    expect(screen.getByText(/Instructions & Agents/i)).toHaveTextContent("(1)");
  });

  it("hides capability sections with no items when graph has mixed types", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Skill 1" },
        { id: "hook-1", kind: "node", artifact_type: "script", name: "Hook 1" },
        { id: "agent-1", kind: "node", artifact_type: "agent", name: "Agent 1" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText(/Skills/i)).toBeInTheDocument();
    expect(screen.getByText(/Hooks & Scripts/i)).toBeInTheDocument();
    expect(screen.queryByText(/MCP Servers/i)).not.toBeInTheDocument();
    expect(screen.getByText(/Instructions & Agents/i)).toBeInTheDocument();
  });

  it("renders a single section with correct count when only one type is present", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "mcp-1", kind: "node", artifact_type: "mcp", name: "MCP 1" },
        { id: "mcp-2", kind: "node", artifact_type: "mcp", name: "MCP 2" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.queryByText(/Skills/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Hooks & Scripts/i)).not.toBeInTheDocument();
    expect(screen.getByText(/MCP Servers/i)).toHaveTextContent("(2)");
    expect(screen.queryByText(/Instructions & Agents/i)).not.toBeInTheDocument();
  });
});
