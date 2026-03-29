import { type InspectTreeNode } from "../lib/inspect";

type InspectTreeProps = {
  expandedKeys: string[];
  tree: InspectTreeNode[];
  selectedNodeId: string;
  onSelect: (nodeId: string) => void;
  onToggleExpand: (key: string) => void;
};

export function InspectTree({
  expandedKeys,
  tree,
  selectedNodeId,
  onSelect,
  onToggleExpand,
}: InspectTreeProps) {
  const expanded = new Set(expandedKeys);

  return (
    <div className="tree-root">
      {tree.map((node) => (
        <TreeBranch
          key={node.key}
          expanded={expanded}
          node={node}
          depth={0}
          selectedNodeId={selectedNodeId}
          onSelect={onSelect}
          onToggleExpand={onToggleExpand}
        />
      ))}
    </div>
  );
}

function TreeBranch({
  expanded,
  node,
  depth,
  selectedNodeId,
  onSelect,
  onToggleExpand,
}: {
  expanded: Set<string>;
  node: InspectTreeNode;
  depth: number;
  selectedNodeId: string;
  onSelect: (nodeId: string) => void;
  onToggleExpand: (key: string) => void;
}) {
  const selected = node.nodeId === selectedNodeId;
  const isExpanded = expanded.has(node.key);
  const hasChildren = node.children.length > 0;
  const selectLabel = node.nodeId ? `Select ${node.label}` : node.label;

  return (
    <div className="tree-branch">
      <div className="tree-row" style={{ paddingLeft: `${depth * 14}px` }}>
        <div className="tree-entry">
          {hasChildren ? (
            <button
              className="tree-toggle"
              onClick={() => onToggleExpand(node.key)}
              aria-label={isExpanded ? `Collapse ${node.label}` : `Expand ${node.label}`}
            >
              {isExpanded ? "▾" : "▸"}
            </button>
          ) : (
            <span className="tree-toggle-placeholder" aria-hidden="true" />
          )}
          {node.nodeId ? (
            <button
              className={`${selected ? "tree-node active" : "tree-node"} usage-${node.usageState}`}
              onClick={() => onSelect(node.nodeId!)}
              title={node.path}
              aria-label={selectLabel}
            >
              <span className={`tree-node-indicator usage-${node.usageState}`} aria-hidden="true">
                {node.usageState === "used" ? "●" : node.usageState === "broken" ? "!" : "○"}
              </span>
              <span>{node.label}</span>
            </button>
          ) : (
            <button
              className={`tree-group usage-${node.usageState}`}
              onClick={() => onToggleExpand(node.key)}
              title={node.path}
            >
              <span className={`tree-node-indicator usage-${node.usageState}`} aria-hidden="true">
                {node.usageState === "used" ? "●" : node.usageState === "broken" ? "!" : "○"}
              </span>
              <span>{node.label}</span>
            </button>
          )}
        </div>
      </div>
      {hasChildren && isExpanded
        ? node.children.map((child) => (
            <TreeBranch
              key={child.key}
              expanded={expanded}
              node={child}
              depth={depth + 1}
              selectedNodeId={selectedNodeId}
              onSelect={onSelect}
              onToggleExpand={onToggleExpand}
            />
          ))
        : null}
    </div>
  );
}
