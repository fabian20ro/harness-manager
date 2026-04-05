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
    <div className="tree-root" style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
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

  const handleRowClick = () => {
    if (node.nodeId) {
      onSelect(node.nodeId);
    } else if (hasChildren) {
      onToggleExpand(node.key);
    }
  };

  return (
    <div className="tree-branch">
      <div 
        className={`tree-node ${selected ? 'active' : ''}`} 
        style={{ paddingLeft: `${depth * 16 + 8}px` }}
        onClick={handleRowClick}
      >
        <span 
          style={{ 
            width: '16px', 
            display: 'flex', 
            justifyContent: 'center', 
            cursor: 'pointer',
            opacity: hasChildren ? 1 : 0 
          }}
          onClick={(e) => {
            if (hasChildren) {
              e.stopPropagation();
              onToggleExpand(node.key);
            }
          }}
        >
          {hasChildren ? (isExpanded ? "▾" : "▸") : ""}
        </span>
        <span className={`tree-node-indicator usage-${node.usageState}`} style={{ fontSize: '0.6rem' }}>
          {node.usageState === "used" ? "●" : node.usageState === "broken" ? "!" : node.usageState === "proposed" ? "◩" : "○"}
        </span>
        <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
          {node.path.includes('/agents/') && node.path.endsWith('.md') ? "🤖 " : node.path.includes('/skills/') && node.path.endsWith('.md') ? "⚡ " : ""}
          {node.label}
        </span>
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
