use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
    Declared,
    Effective,
    Proposed,
    Observed,
    ReferencedOnly,
    Shadowed,
    Ignored,
    Misleading,
    Inactive,
    Unresolved,
    BrokenReference,
    Installed,
    Configured,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ScopeType {
    GlobalUser,
    ManagedSystem,
    Repo,
    Subdirectory,
    Imported,
    PluginProvided,
    FetchedDocSnapshot,
    RuntimeObserved,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProjectKind {
    GitRepo,
    WorkspaceCandidate,
    PluginPackage,
}

fn default_project_kind() -> ProjectKind {
    ProjectKind::GitRepo
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    DiscoveredIn,
    Loads,
    Imports,
    References,
    Overrides,
    Shadows,
    IgnoredBy,
    AppliesTo,
    Activates,
    FetchedFrom,
    InstalledIn,
    ProvidesArtifact,
    Enables,
    Disables,
    CompatibleWith,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    Instructions,
    Config,
    Hook,
    Mcp,
    Skill,
    Agent,
    PluginManifest,
    PluginDoc,
    PluginAsset,
    LocalDoc,
    RemoteSnapshot,
    ReferenceTarget,
    Directory,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: String,
    pub root_path: String,
    pub display_path: String,
    pub name: String,
    #[serde(default = "default_project_kind")]
    pub kind: ProjectKind,
    #[serde(default)]
    pub discovery_reason: String,
    #[serde(default)]
    pub signal_score: i32,
    pub indexed_at: DateTime<Utc>,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolContext {
    pub id: String,
    pub family: String,
    pub display_name: String,
    pub catalog_version: String,
    pub support_level: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceLink {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnownLocation {
    pub path: String,
    pub scope_type: ScopeType,
    pub artifact_type: ArtifactType,
    pub reason: String,
    #[serde(default)]
    pub states: Vec<NodeState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactRule {
    pub glob: String,
    pub artifact_type: ArtifactType,
    pub reason: String,
    #[serde(default)]
    pub states: Vec<NodeState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectDiscoveryRootStrategy {
    MatchParent,
    LevelsUp { count: usize },
    NearestPluginRoot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectDiscoveryRule {
    pub glob: String,
    pub kind: ProjectKind,
    pub score: i32,
    pub reason: String,
    pub root_strategy: ProjectDiscoveryRootStrategy,
    #[serde(default)]
    pub skip_if_scan_root: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginSystemCatalog {
    pub system: String,
    #[serde(default)]
    pub install_roots: Vec<String>,
    #[serde(default)]
    pub manifest_paths: Vec<String>,
    #[serde(default)]
    pub config_paths: Vec<String>,
    #[serde(default)]
    pub compatibility: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCatalog {
    pub surface: String,
    pub family: String,
    pub display_name: String,
    pub version: String,
    pub support_level: String,
    #[serde(default)]
    pub sources: Vec<SourceLink>,
    #[serde(default)]
    pub known_locations: Vec<KnownLocation>,
    #[serde(default)]
    pub artifact_rules: Vec<ArtifactRule>,
    #[serde(default)]
    pub project_discovery_rules: Vec<ProjectDiscoveryRule>,
    #[serde(default)]
    pub observed_probes: Vec<String>,
    #[serde(default)]
    pub plugin_system: Option<PluginSystemCatalog>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactNode {
    pub id: String,
    pub path: String,
    pub display_path: String,
    pub artifact_type: ArtifactType,
    pub tool_family: String,
    pub scope_type: ScopeType,
    pub states: Vec<NodeState>,
    pub confidence: f32,
    pub origin: String,
    pub last_indexed_at: DateTime<Utc>,
    pub hash: String,
    pub mtime: Option<DateTime<Utc>>,
    #[serde(default)]
    pub byte_size: u64,
    pub reason: String,
    pub metadata: Option<JsonValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginNode {
    pub id: String,
    pub name: String,
    pub plugin_system: String,
    pub install_root: String,
    pub display_path: String,
    pub manifest_path: Option<String>,
    #[serde(default)]
    pub discovery_sources: Vec<String>,
    pub states: Vec<NodeState>,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginArtifactNode {
    pub id: String,
    pub plugin_id: String,
    pub path: String,
    pub display_path: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub metadata: Option<JsonValue>,
    pub resolve_from_path: Option<String>,
    pub artifact_type: ArtifactType,
    pub states: Vec<NodeState>,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectNode {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub display_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolContextNode {
    pub id: String,
    pub tool: ToolContext,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSnapshotNode {
    pub id: String,
    pub url: String,
    pub fetched_at: DateTime<Utc>,
    pub content_path: String,
    pub normalized_hash: String,
    pub linked_urls: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphNode {
    Project(ProjectNode),
    ToolContext(ToolContextNode),
    Artifact(ArtifactNode),
    Plugin(PluginNode),
    PluginArtifact(PluginArtifactNode),
    RemoteSnapshot(RemoteSnapshotNode),
}

impl GraphNode {
    pub fn id(&self) -> &str {
        match self {
            GraphNode::Project(node) => &node.id,
            GraphNode::ToolContext(node) => &node.id,
            GraphNode::Artifact(node) => &node.id,
            GraphNode::Plugin(node) => &node.id,
            GraphNode::PluginArtifact(node) => &node.id,
            GraphNode::RemoteSnapshot(node) => &node.id,
        }
    }

    pub fn label(&self) -> String {
        match self {
            GraphNode::Project(node) => node.name.clone(),
            GraphNode::ToolContext(node) => node.tool.display_name.clone(),
            GraphNode::Artifact(node) => node.display_path.clone(),
            GraphNode::Plugin(node) => node.name.clone(),
            GraphNode::PluginArtifact(node) => node
                .name
                .clone()
                .unwrap_or_else(|| node.display_path.clone()),
            GraphNode::RemoteSnapshot(node) => node.url.clone(),
        }
    }

    pub fn states(&self) -> Vec<NodeState> {
        match self {
            GraphNode::Artifact(node) => node.states.clone(),
            GraphNode::Plugin(node) => node.states.clone(),
            GraphNode::PluginArtifact(node) => node.states.clone(),
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub edge_type: EdgeType,
    pub hardness: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Verdict {
    pub entity_id: String,
    pub states: Vec<NodeState>,
    pub why_included: Vec<String>,
    pub why_excluded: Vec<String>,
    pub shadowed_by: Vec<String>,
    pub provenance_paths: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObservationEvidence {
    pub entity_id: String,
    pub source_type: String,
    pub captured_at: DateTime<Utc>,
    pub payload_ref: String,
    pub confidence: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSnapshot {
    pub id: String,
    pub url: String,
    pub fetched_at: DateTime<Utc>,
    pub content_path: String,
    pub normalized_hash: String,
    pub linked_urls: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditableMetadata {
    pub editable: bool,
    pub edit_path: Option<String>,
    pub version_token: Option<String>,
    pub last_saved_backup_available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InspectPayload {
    pub entity: GraphNode,
    pub verdict: Option<Verdict>,
    pub incoming_edges: Vec<GraphEdge>,
    pub outgoing_edges: Vec<GraphEdge>,
    pub related_activity: Vec<ObservationEvidence>,
    pub viewer_content: Option<String>,
    pub edit: EditableMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SurfaceState {
    pub project: ProjectSummary,
    pub tool: ToolContext,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub verdicts: Vec<Verdict>,
    pub last_indexed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotAssociation {
    pub project_id: String,
    pub tool: String,
    pub snapshot: RemoteSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobStatus {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub message: String,
    pub scope_kind: Option<String>,
    pub project_id: Option<String>,
    pub tool: Option<String>,
    pub phase: Option<String>,
    pub current_path: Option<String>,
    pub items_done: Option<usize>,
    pub items_total: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveInspectResponse {
    pub inspect: InspectPayload,
    pub graph: SurfaceState,
    pub status_message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditBackup {
    pub path: String,
    pub content: String,
    pub version_token: String,
}
