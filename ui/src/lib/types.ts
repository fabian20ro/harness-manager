export type ProjectSummary = {
  id: string;
  root_path: string;
  display_path: string;
  name: string;
  kind?: "git_repo" | "workspace_candidate" | "plugin_package";
  discovery_reason?: string;
  signal_score?: number;
  indexed_at: string;
  status: string;
};

type ToolContext = {
  id: string;
  display_name: string;
  support_level: string;
};

export type GraphNodeRecord = {
  id: string;
  kind: string;
  artifact_type?: string;
  path?: string;
  display_path?: string;
  root_path?: string;
  name?: string;
  byte_size?: number;
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
  edit: {
    editable: boolean;
    edit_path?: string;
    version_token?: string;
    last_saved_backup_available: boolean;
  };
};

export type SaveInspectResponse = {
  inspect: InspectPayload;
  graph: SurfaceState;
  status_message: string;
};

export type JobStatus = {
  id: string;
  kind: string;
  status: string;
  created_at: string;
  finished_at?: string | null;
  message: string;
  scope_kind?: string | null;
  project_id?: string | null;
  tool?: string | null;
  phase?: string | null;
  current_path?: string | null;
  items_done?: number | null;
  items_total?: number | null;
};
