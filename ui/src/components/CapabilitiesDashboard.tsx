import { useMemo } from "react";
import { SurfaceState, GraphNodeRecord } from "../lib/types";
import { getNodeLabel, usageStateForStates } from "../lib/inspect";

interface CapabilitiesDashboardProps {
  graph: SurfaceState | null;
}

export function CapabilitiesDashboard({ graph }: CapabilitiesDashboardProps) {
  if (!graph) {
    return (
      <div className="panel" style={{ textAlign: 'center', padding: '40px' }}>
        <p style={{ color: 'var(--muted)' }}>No project or tool context selected.</p>
      </div>
    );
  }

  const nodes = graph.nodes;
  const verdicts = graph.verdicts;

  const skills = useMemo(() => nodes.filter(n => n.artifact_type === 'skill'), [nodes]);
  const hooks = useMemo(() => nodes.filter(n => n.artifact_type === 'hook' || n.artifact_type === 'script'), [nodes]);
  const mcpServers = useMemo(() => nodes.filter(n => n.artifact_type === 'mcp'), [nodes]);
  const instructions = useMemo(() => nodes.filter(n => n.artifact_type === 'instructions' || n.artifact_type === 'agent'), [nodes]);

  return (
    <div style={{ display: 'grid', gap: '24px' }}>
      <CapabilitySection title="Skills" items={skills} verdicts={verdicts} emoji="🛠️" />
      <CapabilitySection title="Hooks & Scripts" items={hooks} verdicts={verdicts} emoji="⚓" />
      <CapabilitySection title="MCP Servers" items={mcpServers} verdicts={verdicts} emoji="🔌" />
      <CapabilitySection title="Instructions & Agents" items={instructions} verdicts={verdicts} emoji="📖" />
    </div>
  );
}

function CapabilitySection({ title, items, verdicts, emoji }: {
  title: string;
  items: GraphNodeRecord[];
  verdicts: SurfaceState["verdicts"];
  emoji: string;
}) {
  if (items.length === 0) return null;

  return (
    <section>
      <h3 style={{ fontSize: '1rem', marginBottom: '12px', display: 'flex', alignItems: 'center', gap: '8px' }}>
        <span>{emoji}</span> {title}
        <span style={{ fontSize: '0.8rem', fontWeight: 'normal', color: 'var(--muted)' }}>({items.length})</span>
      </h3>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: '12px' }}>
        {items.map(item => {
          const states = verdicts.find(v => v.entity_id === item.id)?.states ?? [];
          const usage = usageStateForStates(states);
          const description = (item.description as string) || (item.reason as string);

          return (
            <div key={item.id} className="project-card" style={{ cursor: 'default', borderLeft: `4px solid var(--${statusColor(usage)})` }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                <strong style={{ fontSize: '0.9rem' }}>{getNodeLabel(item)}</strong>
                <StatusBadge usage={usage} />
              </div>
              <p style={{ fontSize: '0.8rem', marginTop: '8px', color: 'var(--muted)', display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical', overflow: 'hidden' }}>
                {description}
              </p>
              {item.path && (
                <code style={{ fontSize: '0.7rem', marginTop: '8px', display: 'block', opacity: 0.6 }}>
                  {item.display_path || item.path}
                </code>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function StatusBadge({ usage }: { usage: string }) {
  const labels: Record<string, string> = {
    used: "Effective",
    unused: "Inactive",
    broken: "Broken",
    proposed: "Proposed"
  };

  return (
    <span style={{
      fontSize: '0.65rem',
      padding: '2px 6px',
      borderRadius: '4px',
      background: `var(--${statusColor(usage)}-bg)`,
      color: `var(--${statusColor(usage)})`,
      fontWeight: 'bold',
      textTransform: 'uppercase'
    }}>
      {labels[usage]}
    </span>
  );
}

function statusColor(usage: string) {
  switch (usage) {
    case 'used': return 'primary';
    case 'broken': return 'warning';
    case 'proposed': return 'accent';
    default: return 'muted';
  }
}

