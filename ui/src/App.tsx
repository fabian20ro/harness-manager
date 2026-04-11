import { InspectReasonsPane } from "./components/InspectReasonsPane";
import { InspectToolbar } from "./components/InspectToolbar";
import { InspectTree } from "./components/InspectTree";
import { ScanStatusBar } from "./components/ScanStatusBar";
import { SidebarNav } from "./components/SidebarNav";
import { ViewerPane } from "./components/ViewerPane";
import { CapabilitiesDashboard } from "./components/CapabilitiesDashboard";
import { formatDisplayPath, getNodeLabel, LABELS } from "./lib/inspect";
import { projectKindLabel } from "./lib/projects";
import { useInspectController } from "./hooks/useInspectController";
import { type HealthReport } from "./lib/types";

export function App() {
  const controller = useInspectController();
  const inspectMode = controller.activeTab === "Inspect";
  const expandedKeySet = new Set(controller.treeExpandedKeys);
  const hasExpandableTree = controller.allTreeKeys.length > 0;
  const treeFullyExpanded =
    hasExpandableTree && controller.allTreeKeys.every((key) => expandedKeySet.has(key));
  const treeFullyCollapsed = controller.treeExpandedKeys.length === 0;
  const inlineStatusMessage =
    controller.inspectStatusMessage || (!controller.scanJob ? controller.statusMessage : "");

  return (
    <div className={controller.sidebarCollapsed ? "shell shell-collapsed" : "shell"}>
      <SidebarNav
        navigation={{
          activeTab: controller.activeTab,
          onSelectTab: controller.setActiveTab,
        }}
        collapse={{
          collapsed: controller.sidebarCollapsed,
          onToggleCollapse: () => controller.setSidebarCollapsed((value) => !value),
        }}
        reindex={{
          label: controller.isGlobalScanRunning ? "Reindexing..." : "Reindex all",
          isRunning: controller.isGlobalScanRunning,
          onReindex: () => void controller.runGlobalScan().catch(() => {}),
        }}
      />

      <main className={inspectMode ? "workspace inspect-mode" : "workspace"}>
        <InspectToolbar
          apiBase={controller.apiBase}
          onApiBaseChange={controller.setApiBase}
          onCopyHelper={() => void controller.copyHelperCommand()}
          onScopedReindex={() => void controller.runScopedReindex().catch(() => {})}
          projects={controller.projects}
          scopedJob={controller.scopedScanJob}
          selectedProject={controller.selectedProject}
          selectedTool={controller.selectedTool}
          onSelectProject={controller.setSelectedProject}
          onSelectTool={controller.setSelectedTool}
          scopedReindexDisabled={!controller.selectedProject || controller.isScopedReindexRunning}
        />

        {controller.staleBuildMessage ? (
          <div className="stale-banner" style={{ margin: '0 24px 16px', marginTop: '16px' }}>
            {controller.staleBuildMessage}
          </div>
        ) : null}
        
        <div style={{ padding: inspectMode ? '0' : '24px' }}>
          {!inspectMode && (
            <ScanStatusBar job={controller.scanJob} message={inlineStatusMessage} graph={controller.graph} />
          )}

          {controller.activeTab === "Projects" && (
            <section>
              <h1 style={{ fontSize: '1.25rem', marginBottom: '20px' }}>Projects</h1>
              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))', gap: '16px' }}>
                {controller.projects.map((project) => (
                  <button
                    key={project.id}
                    className={
                      project.id === controller.selectedProject ? "project-card active" : "project-card"
                    }
                    onClick={() => controller.setSelectedProject(project.id)}
                  >
                    <strong style={{ fontSize: '1rem' }}>{project.name}</strong>
                    <span style={{ fontSize: '0.85rem', color: 'var(--muted)' }}>{projectKindLabel(project)}</span>
                    <span style={{ fontSize: '0.8rem', opacity: 0.8 }}>{formatDisplayPath(project.display_path)}</span>
                    {project.discovery_reason ? <span style={{ fontSize: '0.75rem', fontStyle: 'italic' }}>{project.discovery_reason}</span> : null}
                    <em style={{ fontSize: '0.7rem', marginTop: '4px' }}>{new Date(project.indexed_at).toLocaleString()}</em>
                  </button>
                ))}
              </div>
            </section>
          )}

          {controller.activeTab === "Docs" && (
            <section className="panel">
              <h2>Documentation Snapshot</h2>
              <div className="docs-form" style={{ marginTop: '16px' }}>
                <input
                  value={controller.docUrl}
                  onChange={(event) => controller.setDocUrl(event.target.value)}
                  placeholder="https://docs.example.com"
                />
                <button 
                  className="toolbar-reindex"
                  onClick={() => void controller.fetchDocs()}
                >
                  Fetch snapshot
                </button>
              </div>
              <p style={{ marginTop: '12px', fontSize: '0.85rem', color: 'var(--muted)' }}>
                Snapshot binds to the currently selected project and tool context.
              </p>
            </section>
          )}

          {controller.activeTab === "Tool" && (
            <section className="panel">
              <h2>Active Tool Context</h2>
              <div style={{ marginTop: '16px', display: 'grid', gap: '8px' }}>
                <p><strong>Tool:</strong> {LABELS[controller.selectedTool as keyof typeof LABELS] ?? controller.selectedTool}</p>
                <p><strong>Project:</strong> {controller.selectedProjectMeta?.display_path ?? "No project selected"}</p>
              </div>
            </section>
          )}

          {inspectMode && (
            <div className="inspect-grid">
              <div className="inspect-panel">
                <div className="inspect-panel-header">
                  <h2>Context Tree</h2>
                  <div className="inspect-panel-actions">
                    <button
                      className="panel-action-button"
                      onClick={() => controller.expandAllTree()}
                      disabled={!hasExpandableTree || treeFullyExpanded}
                    >
                      Expand
                    </button>
                    <button
                      className="panel-action-button"
                      onClick={() => controller.collapseAllTree()}
                      disabled={!hasExpandableTree || treeFullyCollapsed}
                    >
                      Collapse
                    </button>
                  </div>
                </div>
                <div className="inspect-panel-body">
                  <InspectTree
                    expandedKeys={controller.treeExpandedKeys}
                    tree={controller.tree}
                    selectedNodeId={controller.selectedNode}
                    onSelect={controller.setSelectedNode}
                    onToggleExpand={controller.toggleExpandedKey}
                  />
                </div>
              </div>

              <div className="inspect-panel">
                <div className="inspect-panel-header">
                  <h2>{controller.currentNode ? getNodeLabel(controller.currentNode) : "Viewer"}</h2>
                  {controller.inspectStatusMessage && (
                    <span style={{ fontSize: '0.75rem', color: 'var(--warning)' }}>{controller.inspectStatusMessage}</span>
                  )}
                </div>
                <div className="inspect-panel-body" style={{ padding: 0 }}>
                  <ViewerPane
                    nodeKey={controller.selectedNode}
                    content={controller.inspect?.viewer_content}
                    metadata={controller.currentNode?.metadata as Record<string, unknown> | undefined}
                    health={controller.currentNode?.health as HealthReport | undefined}
                    editable={controller.inspect?.edit.editable}
                    versionToken={controller.inspect?.edit.version_token}
                    lastSavedBackupAvailable={controller.inspect?.edit.last_saved_backup_available}
                    onSave={controller.saveInspectContent}
                    onReload={controller.reloadInspectNode}
                    onRevert={controller.revertInspectSave}
                    onFix={controller.fixInspectCheck}
                  />
                </div>
              </div>

              <div className="inspect-panel">
                <div className="inspect-panel-header">
                  <h2>Reasoning & Metadata</h2>
                </div>
                <div className="inspect-panel-body">
                  <InspectReasonsPane
                    currentNode={controller.currentNode}
                    inspect={controller.inspect}
                  />
                </div>
              </div>
            </div>
          )}

          {controller.activeTab === "Capabilities" && (
            <CapabilitiesDashboard graph={controller.graph} />
          )}

          {controller.activeTab === "Activity" && (
            <section className="panel">
              <div className="activity-header">
                <h2>Recent Activity</h2>
                <button 
                  className="panel-action-button"
                  onClick={() => void controller.refreshActivity()}
                >
                  Refresh
                </button>
              </div>
              <ul style={{ marginTop: '16px', display: 'grid', gap: '8px', listStyle: 'none', padding: 0 }}>
                {controller.inspect?.related_activity.map((item) => (
                  <li key={item.payload_ref} style={{ padding: '8px 12px', background: 'var(--bg)', borderRadius: '6px', fontSize: '0.85rem' }}>
                    <code style={{ color: 'var(--primary)' }}>{item.payload_ref}</code> 
                    <span style={{ marginLeft: '12px', color: 'var(--muted)' }}>Confidence: {item.confidence.toFixed(2)}</span>
                  </li>
                ))}
                {(!controller.inspect?.related_activity || controller.inspect.related_activity.length === 0) && (
                  <li style={{ color: 'var(--muted)', fontStyle: 'italic' }}>No recent activity observed.</li>
                )}
              </ul>
            </section>
          )}
        </div>
      </main>
    </div>
  );
}
