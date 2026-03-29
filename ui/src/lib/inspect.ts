import type { GraphNodeRecord, SurfaceState } from "./types";

export const HELPER_COMMAND = "cargo run";

export const TOOL_IDS = [
  "claude_code",
  "claude_cowork",
  "codex",
  "codex_cli",
  "copilot_cli",
  "intellij_copilot",
  "opencode",
  "antigravity",
] as const;

export const LABELS: Record<(typeof TOOL_IDS)[number], string> = {
  claude_code: "Claude Code",
  claude_cowork: "Claude Cowork",
  codex: "Codex",
  codex_cli: "Codex CLI",
  copilot_cli: "Copilot CLI",
  intellij_copilot: "IntelliJ/Copilot",
  opencode: "OpenCode",
  antigravity: "Antigravity",
};

export const MENU_ITEMS = [
  { id: "Projects", label: "Projects", emoji: "📁" },
  { id: "Docs", label: "Docs", emoji: "📚" },
  { id: "Tool", label: "Tool", emoji: "🛠️" },
  { id: "Inspect", label: "Inspect", emoji: "🔎" },
  { id: "Activity", label: "Activity", emoji: "⚡" },
] as const;

export type AppTab = (typeof MENU_ITEMS)[number]["id"];

export type InspectTreeNode = {
  key: string;
  label: string;
  nodeId?: string;
  path?: string;
  children: InspectTreeNode[];
  states: string[];
  usageState: "used" | "unused" | "broken";
  score: number;
};

type TrieNode = {
  segment: string;
  nodeId?: string;
  path?: string;
  states: string[];
  children: Map<string, TrieNode>;
};

export function formatDisplayPath(path?: string) {
  if (!path) return "";
  if (path === "~" || path === "/") return path;
  if (path.startsWith("~/")) return path;
  return path.replace(/\/{2,}/g, "/");
}

export function getNodeDisplayPath(node: GraphNodeRecord) {
  return formatDisplayPath(node.display_path ?? node.path ?? node.root_path ?? "");
}

export function getNodeLabel(node: GraphNodeRecord) {
  return node.name || getNodeDisplayPath(node) || node.id;
}

export function pickNextSelectedNode(
  previousNodeId: string | null | undefined,
  nodes: GraphNodeRecord[],
  verdicts: SurfaceState["verdicts"],
) {
  if (previousNodeId && nodes.some((node) => node.id === previousNodeId)) {
    return previousNodeId;
  }

  const byPriority = [...nodes]
    .filter((node) => node.kind !== "tool_context")
    .sort((left, right) => scoreNode(left, verdicts) - scoreNode(right, verdicts));

  return byPriority[0]?.id ?? "";
}

export function buildInspectTree(graph: SurfaceState | null): InspectTreeNode[] {
  if (!graph) return [];

  const rootMap = new Map<string, TrieNode>();
  const entries = [
    { nodeId: `project:${graph.project.id}`, path: graph.project.display_path, states: ["effective"] },
    ...graph.nodes
      .filter((node) => node.kind !== "project")
      .map((node) => ({
        nodeId: node.id,
        path: getNodeDisplayPath(node),
        states: graph.verdicts.find((verdict) => verdict.entity_id === node.id)?.states ?? [],
      }))
      .filter((entry) => isPathBearing(entry.path)),
  ];

  for (const entry of entries) {
    const parsed = splitDisplayPath(entry.path);
    if (!parsed) continue;
    const root = ensureChild(rootMap, parsed.root, parsed.root);
    let current = root;
    for (const segment of parsed.segments) {
      current = ensureChild(current.children, segment, segment);
    }
    current.nodeId = entry.nodeId;
    current.path = entry.path;
    current.states = entry.states;
  }

  return [...rootMap.values()]
    .map((root) => compressTrie(root))
    .sort((left, right) => left.label.localeCompare(right.label));
}

function isPathBearing(path: string) {
  return Boolean(path) && (path.startsWith("~/") || path.startsWith("/"));
}

function splitDisplayPath(path: string) {
  if (path === "~") return { root: "~", segments: [] as string[] };
  if (path.startsWith("~/")) {
    return {
      root: "~",
      segments: path.slice(2).split("/").filter(Boolean),
    };
  }
  if (path.startsWith("/")) {
    return {
      root: "/",
      segments: path.split("/").filter(Boolean),
    };
  }
  return null;
}

function ensureChild(map: Map<string, TrieNode>, key: string, segment: string) {
  const existing = map.get(key);
  if (existing) return existing;
  const created: TrieNode = {
    segment,
    states: [],
    children: new Map<string, TrieNode>(),
  };
  map.set(key, created);
  return created;
}

function compressTrie(node: TrieNode): InspectTreeNode {
  let label = node.segment;
  let current = node;

  while (!current.nodeId && current.children.size === 1) {
    const child = [...current.children.values()][0];
    label = `${label}/${child.segment}`;
    current = child;
  }

  const children = [...current.children.values()]
    .map((child) => compressTrie(child))
    .sort((left, right) => left.score - right.score || left.label.localeCompare(right.label));

  return {
    key: `${label}:${current.nodeId ?? label}`,
    label,
    nodeId: current.nodeId,
    path: current.path,
    children,
    states: current.states,
    usageState: usageStateForStates(current.states),
    score: scoreStates(current.states),
  };
}

function scoreNode(node: GraphNodeRecord, verdicts: SurfaceState["verdicts"]) {
  return scoreStates(verdicts.find((verdict) => verdict.entity_id === node.id)?.states ?? []);
}

function scoreStates(states: string[]) {
  if (states.includes("effective")) return 0;
  if (states.includes("misleading")) return 1;
  if (states.includes("referenced_only")) return 2;
  if (states.includes("shadowed")) return 3;
  return 4;
}

export function usageStateForStates(states: string[]): "used" | "unused" | "broken" {
  if (states.includes("broken_reference") || states.includes("unresolved")) {
    return "broken";
  }
  if (states.includes("effective") || states.includes("observed")) {
    return "used";
  }
  return "unused";
}
