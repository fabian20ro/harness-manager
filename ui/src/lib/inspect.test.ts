import { describe, expect, it } from "vitest";

import { buildInspectTree, formatDisplayPath, pickNextSelectedNode } from "./inspect";
import type { SurfaceState } from "./types";

const graph: SurfaceState = {
  project: {
    id: "p1",
    root_path: "/Users/fabian/git/harness-manager",
    display_path: "~/git/harness-manager",
    name: "harness-manager",
    indexed_at: new Date().toISOString(),
    status: "ready",
  },
  tool: {
    id: "codex",
    display_name: "Codex",
    support_level: "full",
  },
  nodes: [
    {
      id: "tool:codex",
      kind: "tool_context",
    },
    {
      id: "global-config",
      kind: "artifact",
      path: "/Users/fabian/.codex/config.toml",
      display_path: "~/.codex/config.toml",
    },
    {
      id: "repo-file",
      kind: "artifact",
      path: "/Users/fabian/git/harness-manager/AGENTS.md",
      display_path: "~/git/harness-manager/AGENTS.md",
    },
  ],
  edges: [],
  verdicts: [
    { entity_id: "global-config", states: ["effective"], why_included: [], why_excluded: [] },
    { entity_id: "repo-file", states: ["referenced_only"], why_included: [], why_excluded: [] },
  ],
};

describe("inspect helpers", () => {
  it("preserves home-relative paths", () => {
    expect(formatDisplayPath("~/git/harness-manager")).toBe("~/git/harness-manager");
  });

  it("builds a compact tree rooted at ~", () => {
    const tree = buildInspectTree(graph);
    expect(tree[0]?.label).toBe("~");
    expect(tree[0]?.children.map((child) => child.label)).toContain(".codex/config.toml");
    expect(tree[0]?.children.map((child) => child.label)).toContain("git/harness-manager");
  });

  it("preserves selected node when still present", () => {
    expect(pickNextSelectedNode("repo-file", graph.nodes, graph.verdicts)).toBe("repo-file");
  });

  it("falls back deterministically to the highest-priority node", () => {
    expect(pickNextSelectedNode("missing", graph.nodes, graph.verdicts)).toBe("global-config");
  });
});
