import { type InspectTreeNode } from "../lib/inspect";

type InspectTreeProps = {
  tree: InspectTreeNode[];
  selectedNodeId: string;
  onSelect: (nodeId: string) => void;
};

export function InspectTree({ tree, selectedNodeId, onSelect }: InspectTreeProps) {
  return (
    <div className="tree-root">
      {tree.map((node) => (
        <TreeBranch
          key={node.key}
          node={node}
          depth={0}
          selectedNodeId={selectedNodeId}
          onSelect={onSelect}
        />
      ))}
    </div>
  );
}

function TreeBranch({
  node,
  depth,
  selectedNodeId,
  onSelect,
}: {
  node: InspectTreeNode;
  depth: number;
  selectedNodeId: string;
  onSelect: (nodeId: string) => void;
}) {
  const selected = node.nodeId === selectedNodeId;
  return (
    <div className="tree-branch">
      <div className="tree-row" style={{ paddingLeft: `${depth * 14}px` }}>
        {node.nodeId ? (
          <button
            className={selected ? "tree-node active" : "tree-node"}
            onClick={() => onSelect(node.nodeId!)}
            title={node.path}
          >
            {node.label}
          </button>
        ) : (
          <div className="tree-group">{node.label}</div>
        )}
      </div>
      {node.children.map((child) => (
        <TreeBranch
          key={child.key}
          node={child}
          depth={depth + 1}
          selectedNodeId={selectedNodeId}
          onSelect={onSelect}
        />
      ))}
    </div>
  );
}
