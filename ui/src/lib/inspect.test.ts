import { describe, expect, it } from "vitest";

import {
  buildInspectTree,
  collectAllDirectoryKeys,
  collectSelectedAncestorKeys,
  formatDisplayPath,
  pickNextSelectedNode,
  usageStateForStates,
} from "./inspect";
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
    {
      id: "plugin-skill",
      kind: "plugin_artifact",
      path: "/Users/fabian/.codex/.tmp/plugins/plugins/vercel/skills/nextjs/SKILL.md",
      display_path: "~/.codex/.tmp/plugins/plugins/vercel/skills/nextjs/SKILL.md",
      name: "Next.js App Router",
    },
  ],
  edges: [],
  verdicts: [
    { entity_id: "global-config", states: ["effective"], why_included: [], why_excluded: [] },
    { entity_id: "repo-file", states: ["referenced_only"], why_included: [], why_excluded: [] },
    { entity_id: "plugin-skill", states: ["effective"], why_included: [], why_excluded: [] },
  ],
};

describe("inspect helpers", () => {
  it("preserves home-relative paths", () => {
    expect(formatDisplayPath("~/git/harness-manager")).toBe("~/git/harness-manager");
  });

  it("builds an explicit tree rooted at ~", () => {
    const tree = buildInspectTree(graph);
    expect(tree[0]?.label).toBe("~");
    expect(tree[0]?.children.map((child) => child.label)).toContain(".codex");
    expect(tree[0]?.children.map((child) => child.label)).toContain("git");
  });

  it("collects all directory keys for default expansion", () => {
    const tree = buildInspectTree(graph);
    expect(collectAllDirectoryKeys(tree)).toEqual(
      expect.arrayContaining(["~", "~/git", "~/git/harness-manager", "~/.codex"]),
    );
  });

  it("collects selected node ancestors for one-shot auto expansion", () => {
    const tree = buildInspectTree(graph);
    expect(collectSelectedAncestorKeys(tree, "repo-file")).toEqual(
      expect.arrayContaining(["~", "~/git", "~/git/harness-manager"]),
    );
  });

  it("prefers named plugin skill labels over raw SKILL.md leaf names", () => {
    const tree = buildInspectTree(graph);
    const codexRoot = tree[0]?.children.find((child) => child.label === ".codex");
    const skillLeaf = codexRoot
      ?.children.find((child) => child.label === ".tmp")
      ?.children.find((child) => child.label === "plugins")
      ?.children.find((child) => child.label === "plugins")
      ?.children.find((child) => child.label === "vercel")
      ?.children.find((child) => child.label === "skills")
      ?.children.find((child) => child.label === "nextjs")
      ?.children[0];
    expect(skillLeaf?.label).toBe("Next.js App Router");
  });

  it("preserves selected node when still present", () => {
    expect(pickNextSelectedNode("repo-file", graph.nodes, graph.verdicts)).toBe("repo-file");
  });

  it("falls back deterministically to the highest-priority node", () => {
    expect(pickNextSelectedNode("missing", graph.nodes, graph.verdicts)).toBe("global-config");
  });

  it("derives usage state from verdict states", () => {
    expect(usageStateForStates(["effective"])).toBe("used");
    expect(usageStateForStates(["observed"])).toBe("used");
    expect(usageStateForStates(["referenced_only"])).toBe("unused");
    expect(usageStateForStates(["broken_reference"])).toBe("broken");
  });
});
