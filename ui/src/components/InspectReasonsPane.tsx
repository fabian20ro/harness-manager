import { formatDisplayPath } from "../lib/inspect";
import type { GraphNodeRecord, InspectPayload } from "../lib/types";

type InspectReasonsPaneProps = {
  currentNode: GraphNodeRecord | null;
  inspect: InspectPayload | null;
};

export function InspectReasonsPane({ currentNode, inspect }: InspectReasonsPaneProps) {
  return (
    <div className="inspect-panel-stack">
      {currentNode?.display_path ? (
        <p className="inspect-path">{formatDisplayPath(String(currentNode.display_path))}</p>
      ) : null}
      <div className="badge-row">
        {inspect?.verdict?.states.map((state) => (
          <span key={state} className={`badge badge-${state}`}>
            {state}
          </span>
        ))}
      </div>
      <section>
        <h3>Why in</h3>
        <ul>
          {inspect?.verdict?.why_included.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      </section>
      <section>
        <h3>Why out</h3>
        <ul>
          {inspect?.verdict?.why_excluded.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      </section>
      <section>
        <h3>References out</h3>
        <ul>
          {inspect?.outgoing_edges.map((edge) => (
            <li key={`${edge.to}-${edge.edge_type}`}>
              {edge.edge_type}: {edge.reason}
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
