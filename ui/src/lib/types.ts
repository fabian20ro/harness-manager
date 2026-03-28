export type ProjectSummary = {
  id: string;
  root_path: string;
  display_path: string;
  name: string;
  indexed_at: string;
  status: string;
};

export type ToolContext = {
  id: string;
  display_name: string;
  support_level: string;
};

export type GraphNodeRecord = {
  id: string;
  kind: string;
  path?: string;
  display_path?: string;
  root_path?: string;
  name?: string;
  [key: string]: unknown;
};

export type SurfaceState = {
  project: ProjectSummary;
  tool: ToolContext;
  nodes: GraphNodeRecord[];
  edges: Array<{ from: string; to: string; edge_type: string; reason: string }>;
  verdicts: Array<{
    entity_id: string;
    states: string[];
    why_included: string[];
    why_excluded: string[];
  }>;
};

export type InspectPayload = {
  entity: GraphNodeRecord;
  verdict?: {
    states: string[];
    why_included: string[];
    why_excluded: string[];
    shadowed_by: string[];
  };
  incoming_edges: Array<{ from: string; edge_type: string; reason: string }>;
  outgoing_edges: Array<{ to: string; edge_type: string; reason: string }>;
  related_activity: Array<{ payload_ref: string; confidence: number }>;
  viewer_content?: string;
};
