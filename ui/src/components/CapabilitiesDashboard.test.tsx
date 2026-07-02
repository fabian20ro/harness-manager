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

  it("renders correct count for each section", () => {
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

  it("renders nodes as Inactive when verdicts do not include their entity_id", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Known Skill" },
        { id: "unknown-skill", kind: "node", artifact_type: "skill", name: "Unknown Skill" },
      ],
      edges: [],
      verdicts: [
        { entity_id: "skill-1", states: ["effective"], why_included: [], why_excluded: [] },
      ],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("Known Skill")).toBeInTheDocument();
    expect(screen.getByText("Unknown Skill")).toBeInTheDocument();
    expect(
      screen.getAllByText(/Inactive/i, { selector: "span" }),
    ).toHaveLength(1);
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

  it("falls back to id when description and reason are absent", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Fallback Skill" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("Fallback Skill")).toBeInTheDocument();
    const card = screen.getByText("Fallback Skill").closest(".project-card");
    const pEl = card?.querySelector("p");
    expect(pEl).not.toHaveTextContent(/description/i);
  });

  it("renders description when present on the node", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Described Skill", description: "A skill with a description" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("A skill with a description")).toBeInTheDocument();
  });

  it("renders display_path in code element, falling back to path when display_path is absent", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-display", kind: "node", artifact_type: "skill", name: "Display Skill", path: "/workspace/skill.ts", display_path: "/other/display.ts" },
        { id: "skill-path", kind: "node", artifact_type: "skill", name: "Path Skill", path: "/workspace/path.ts" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("/other/display.ts", { selector: "code" })).toBeInTheDocument();
    expect(screen.getByText("/workspace/path.ts", { selector: "code" })).toBeInTheDocument();
  });

  it("omits the code element when neither display_path nor path is set on a node", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-no-path", kind: "node", artifact_type: "skill", name: "No Path Skill" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    const card = screen.getByText("No Path Skill").closest(".project-card");
    expect(card?.querySelector("code")).toBeNull();
  });

  it("renders without crashing when a node has no name but has an id", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-no-name", kind: "node", artifact_type: "skill" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    const card = screen.getByText(/skill-no-name/).closest(".project-card");
    expect(card).toBeInTheDocument();
  });

  it("falls back to node.reason as the description text when description is absent", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Reason Skill", reason: "Discovered during index pass" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("Discovered during index pass")).toBeInTheDocument();
  });

  it("renders getNodeDisplayPath output as the strong label when name is absent", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-no-name-display", kind: "node", artifact_type: "skill", display_path: "~/git/harness-manager" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("~/git/harness-manager", { selector: "strong" })).toBeInTheDocument();
  });

  it("renders getNodeDisplayPath output as the strong label when only path is present", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-no-name-path", kind: "node", artifact_type: "skill", path: "/workspace/skill.ts" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("/workspace/skill.ts", { selector: "strong" })).toBeInTheDocument();
  });

  it("renders getNodeDisplayPath output from root_path as the strong label when name, display_path and path are absent", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-no-name-path-root", kind: "node", artifact_type: "skill", root_path: "/workspace/root-only" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("/workspace/root-only", { selector: "strong" })).toBeInTheDocument();
  });

  it("applies the correct border-left CSS variable based on node usage state", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-used", kind: "node", artifact_type: "skill", name: "Used Skill" },
        { id: "skill-broken", kind: "node", artifact_type: "skill", name: "Broken Skill" },
        { id: "skill-proposed", kind: "node", artifact_type: "skill", name: "Proposed Skill" },
      ],
      edges: [],
      verdicts: [
        { entity_id: "skill-used", states: ["effective"], why_included: [], why_excluded: [] },
        { entity_id: "skill-broken", states: ["unresolved"], why_included: [], why_excluded: [] },
        { entity_id: "skill-proposed", states: ["proposed"], why_included: [], why_excluded: [] },
      ],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    const usedCard = screen.getByText("Used Skill").closest(".project-card");
    expect(usedCard).toHaveStyle({ borderLeft: "4px solid var(--primary)" });

    const brokenCard = screen.getByText("Broken Skill").closest(".project-card");
    expect(brokenCard).toHaveStyle({ borderLeft: "4px solid var(--warning)" });

    const proposedCard = screen.getByText("Proposed Skill").closest(".project-card");
    expect(proposedCard).toHaveStyle({ borderLeft: "4px solid var(--accent)" });
  });

  it("uses description over reason when both are present on the node", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Both Skill", description: "The visible one", reason: "The hidden one" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("The visible one")).toBeInTheDocument();
    const card = screen.getByText("Both Skill").closest(".project-card");
    const pEl = card?.querySelector("p");
    expect(pEl).not.toHaveTextContent(/hidden/i);
  });

  it("renders 'observed' verdict states as Effective badges via usageStateForStates", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Observed Skill" },
      ],
      edges: [],
      verdicts: [
        { entity_id: "skill-1", states: ["observed"], why_included: [], why_excluded: [] },
      ],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("Effective")).toBeInTheDocument();
  });

  it.each([
    { usage: "effective", colorVar: "primary", label: "Effective" },
    { usage: "unresolved", colorVar: "warning", label: "Broken" },
    { usage: "proposed", colorVar: "accent", label: "Proposed" },
    { usage: "", colorVar: "muted", label: "Inactive" },
  ])("sets badge background and text color CSS variables for $usage state", ({ usage, colorVar, label }) => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Badge Skill" },
      ],
      edges: [],
      verdicts: [{ entity_id: "skill-1", states: usage ? [usage] : [], why_included: [], why_excluded: [] }],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    const badge = screen.getByText(label, { selector: "span" });
    expect(badge).toHaveStyle({ background: `var(--${colorVar}-bg)` });
    expect(badge).toHaveStyle({ color: `var(${usage ? "--" + colorVar : "--muted"})` });
  });

  it("renders Inactive badge when verdict states array is empty (no verdict for node)", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Empty Verdict Skill" },
      ],
      edges: [],
      verdicts: [{ entity_id: "skill-1", states: [], why_included: [], why_excluded: [] }],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    const badge = screen.getByText("Inactive", { selector: "span" });
    expect(badge).toBeInTheDocument();
  });

  it("maps broken_reference verdict state to Broken badge via usageStateForStates", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Broken Ref Skill" },
      ],
      edges: [],
      verdicts: [{ entity_id: "skill-1", states: ["broken_reference"], why_included: [], why_excluded: [] }],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("Broken")).toBeInTheDocument();
    const card = screen.getByText("Broken Ref Skill").closest(".project-card");
    expect(card).toHaveStyle({ borderLeft: "4px solid var(--warning)" });
  });

  it("renders 'Effective' label when node has multiple verdict states including effective or observed", () => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Multi-State Skill" },
      ],
      edges: [],
      verdicts: [{ entity_id: "skill-1", states: ["effective", "observed"], why_included: [], why_excluded: [] }],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText("Effective")).toBeInTheDocument();
  });

  it.each([
    { title: /Skills/, emoji: "🛠️" },
    { title: /Hooks & Scripts/, emoji: "⚓" },
    { title: /MCP Servers/, emoji: "🔌" },
    { title: /Instructions & Agents/, emoji: "📖" },
  ])("renders the correct $emoji icon in the section header for $title", ({ title, emoji }) => {
    const graph: SurfaceState = {
      project: baseGraph.project,
      tool: baseGraph.tool,
      nodes: [
        { id: "skill-1", kind: "node", artifact_type: "skill", name: "Skill 1" },
        { id: "hook-1", kind: "node", artifact_type: "hook", name: "Hook 1" },
        { id: "mcp-1", kind: "node", artifact_type: "mcp", name: "MCP 1" },
        { id: "instr-1", kind: "node", artifact_type: "instructions", name: "Instr 1" },
      ],
      edges: [],
      verdicts: [],
    };

    render(<CapabilitiesDashboard graph={graph} />);

    expect(screen.getByText(emoji)).toBeInTheDocument();
    const heading = screen.getAllByRole("heading");
    const h3 = heading.find(h => h.textContent?.includes(title.source.slice(1, -1)));
    expect(h3).not.toBeUndefined();
    expect(h3!.textContent).toContain(emoji);
  });
});
