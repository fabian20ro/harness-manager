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
  path: string;
  children: InspectTreeNode[];
  states: string[];
  usageState: "used" | "unused" | "broken" | "proposed";
  score: number;
  isDirectory: boolean;
};

type TrieNode = {
  key: string;
  segment: string;
  displayLabel?: string;
  path: string;
  nodeId?: string;
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

  const verdictsMap = new Map<string, string[]>();
  for (const verdict of verdicts) {
    verdictsMap.set(verdict.entity_id, verdict.states);
  }

  const byPriority = [...nodes]
    .filter((node) => node.kind !== "tool_context")
    .sort((left, right) => scoreStates(verdictsMap.get(left.id) ?? []) - scoreStates(verdictsMap.get(right.id) ?? []));

  return byPriority[0]?.id ?? "";
}

export function buildInspectTree(graph: SurfaceState | null): InspectTreeNode[] {
  if (!graph) return [];

  const verdictsMap = new Map<string, string[]>();
  for (const verdict of graph.verdicts) {
    verdictsMap.set(verdict.entity_id, verdict.states);
  }

  const rootMap = new Map<string, TrieNode>();
  const entries = [
    {
      nodeId: `project:${graph.project.id}`,
      path: graph.project.display_path,
      label: undefined,
      states: ["effective"],
    },
    ...graph.nodes
      .filter((node) => node.kind !== "project")
      .map((node) => ({
        nodeId: node.id,
        path: getNodeDisplayPath(node),
        label: typeof node.name === "string" ? node.name : undefined,
        states: verdictsMap.get(node.id) ?? [],
      }))
      .filter((entry) => isPathBearing(entry.path)),
  ];

  for (const entry of entries) {
    const parsed = splitDisplayPath(entry.path);
    if (!parsed) continue;
    const root = ensureChild(rootMap, parsed.root, parsed.root, parsed.root);
    let current = root;
    let currentPath = parsed.root;
    for (const segment of parsed.segments) {
      currentPath = currentPath === "/" ? `/${segment}` : `${currentPath}/${segment}`;
      current = ensureChild(current.children, currentPath, segment, currentPath);
    }
    current.nodeId = entry.nodeId;
    current.displayLabel = entry.label;
    current.states = entry.states;
  }

  return [...rootMap.values()]
    .map((root) => finalizeTree(root))
    .sort((left, right) => left.label.localeCompare(right.label));
}

export function collectAllDirectoryKeys(tree: InspectTreeNode[]) {
  const expandedKeys = new Set<string>();

  for (const node of tree) {
    collectDirectoryKeys(node, expandedKeys);
  }

  return [...expandedKeys];
}

export function collectSelectedAncestorKeys(tree: InspectTreeNode[], selectedNodeId: string) {
  const ancestorKeys = new Set<string>();

  for (const node of tree) {
    collectSelectedAncestors(node, selectedNodeId, ancestorKeys);
  }

  return [...ancestorKeys];
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

function ensureChild(map: Map<string, TrieNode>, key: string, segment: string, path: string) {
  const existing = map.get(key);
  if (existing) return existing;
  const created: TrieNode = {
    key,
    segment,
    path,
    displayLabel: undefined,
    states: [],
    children: new Map<string, TrieNode>(),
  };
  map.set(key, created);
  return created;
}

function finalizeTree(node: TrieNode): InspectTreeNode {
  const children = [...node.children.values()]
    .map((child) => finalizeTree(child))
    .sort((left, right) => left.score - right.score || left.label.localeCompare(right.label));
  const ownUsageState = usageStateForStates(node.states);
  const childUsageStates = children.map((child) => child.usageState);

  return {
    key: node.key,
    label: node.displayLabel ?? node.segment,
    nodeId: node.nodeId,
    path: node.path,
    children,
    states: node.states,
    usageState:
      ownUsageState === "used" ||
      (!node.states.length && childUsageStates.includes("used"))
        ? "used"
        : ownUsageState === "broken" ||
            (!node.states.length && childUsageStates.includes("broken"))
          ? "broken"
          : ownUsageState === "proposed" ||
              (!node.states.length && childUsageStates.includes("proposed"))
            ? "proposed"
            : "unused",
    score: Math.min(scoreStates(node.states), ...children.map((child) => child.score)),
    isDirectory: children.length > 0,
  };
}

function collectDirectoryKeys(node: InspectTreeNode, expandedKeys: Set<string>) {
  if (!node.isDirectory) {
    return;
  }

  expandedKeys.add(node.key);
  node.children.forEach((child) => collectDirectoryKeys(child, expandedKeys));
}

function collectSelectedAncestors(
  node: InspectTreeNode,
  selectedNodeId: string,
  ancestorKeys: Set<string>,
): boolean {
  const matches = node.nodeId === selectedNodeId;
  const descendantMatches = node.children.some((child) =>
    collectSelectedAncestors(child, selectedNodeId, ancestorKeys),
  );

  if (node.children.length > 0 && descendantMatches) {
    ancestorKeys.add(node.key);
  }

  return matches || descendantMatches;
}

function scoreStates(states: string[]) {
  if (states.includes("effective")) return 0;
  if (states.includes("misleading")) return 1;
  if (states.includes("referenced_only")) return 2;
  if (states.includes("shadowed")) return 3;
  return 4;
}

export function usageStateForStates(states: string[]): "used" | "unused" | "broken" | "proposed" {
  if (states.includes("broken_reference") || states.includes("unresolved")) {
    return "broken";
  }
  if (states.includes("effective") || states.includes("observed")) {
    return "used";
  }
  if (states.includes("proposed")) {
    return "proposed";
  }
  return "unused";
}

export function calculateContextCost(graph: SurfaceState | null): { bytes: number; warning: boolean } {
  if (!graph) return { bytes: 0, warning: false };

  const verdictsMap = new Map<string, string[]>();
  for (const verdict of graph.verdicts) {
    verdictsMap.set(verdict.entity_id, verdict.states);
  }

  const bytes = graph.nodes.reduce((acc, node) => {
    if (typeof node.byte_size === "number") {
      const usage = usageStateForStates(verdictsMap.get(node.id) ?? []);
      if (usage === "used" || usage === "proposed") {
        return acc + node.byte_size;
      }
    }
    return acc;
  }, 0);

  // Gemini context limit warning heuristic: > 200KB of files
  return { bytes, warning: bytes > 200 * 1024 };
}
