import { InspectReasonsPane } from "./components/InspectReasonsPane";
import { InspectToolbar } from "./components/InspectToolbar";
import { InspectTree } from "./components/InspectTree";
import { ScanStatusBar } from "./components/ScanStatusBar";
import { SidebarNav } from "./components/SidebarNav";
import { ViewerPane } from "./components/ViewerPane";
import { formatDisplayPath, getNodeLabel, LABELS } from "./lib/inspect";
import { useInspectController } from "./hooks/useInspectController";

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
        activeTab={controller.activeTab}
        collapsed={controller.sidebarCollapsed}
        globalReindexLabel={controller.isGlobalScanRunning ? "Reindexing all..." : "Reindex all"}
        isGlobalReindexRunning={controller.isGlobalScanRunning}
        onSelectTab={controller.setActiveTab}
        onToggleCollapse={() => controller.setSidebarCollapsed((value) => !value)}
        onReindex={() => void controller.runGlobalScan().catch(() => {})}
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
          <p className="stale-banner">{controller.staleBuildMessage}</p>
        ) : null}
        <ScanStatusBar job={controller.scanJob} message={inlineStatusMessage} />

        {controller.activeTab === "Projects" && (
          <section className="panel">
            <h2>Projects</h2>
            <div className="project-list">
              {controller.projects.map((project) => (
                <button
                  key={project.id}
                  className={
                    project.id === controller.selectedProject ? "project-card active" : "project-card"
                  }
                  onClick={() => controller.setSelectedProject(project.id)}
                >
                  <strong>{project.name}</strong>
                  <span>{formatDisplayPath(project.display_path)}</span>
                  <em>{new Date(project.indexed_at).toLocaleString()}</em>
                </button>
              ))}
            </div>
          </section>
        )}

        {controller.activeTab === "Docs" && (
          <section className="panel">
            <h2>Docs</h2>
            <div className="docs-form">
              <input
                value={controller.docUrl}
                onChange={(event) => controller.setDocUrl(event.target.value)}
              />
              <button onClick={() => void controller.fetchDocs()}>Fetch snapshot</button>
            </div>
            <p>Snapshot binds to selected project and tool context.</p>
          </section>
        )}

        {controller.activeTab === "Tool" && (
          <section className="panel">
            <h2>Tool Context</h2>
            <p>{LABELS[controller.selectedTool as keyof typeof LABELS] ?? controller.selectedTool}</p>
            <p>Project: {controller.selectedProjectMeta?.display_path ?? "No project selected"}</p>
          </section>
        )}

        {inspectMode && (
          <section className="inspect-grid">
            <div className="panel inspect-panel inspect-panel-tree">
              <div className="inspect-panel-header">
                <h2>Effective Context Tree</h2>
                <div className="inspect-panel-actions">
                  <button
                    className="panel-action-button"
                    onClick={() => controller.expandAllTree()}
                    disabled={!hasExpandableTree || treeFullyExpanded}
                  >
                    Expand all
                  </button>
                  <button
                    className="panel-action-button"
                    onClick={() => controller.collapseAllTree()}
                    disabled={!hasExpandableTree || treeFullyCollapsed}
                  >
                    Collapse all
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

            <div className="panel inspect-panel inspect-panel-viewer">
              <h2>{controller.currentNode ? getNodeLabel(controller.currentNode) : "Viewer"}</h2>
              <div className="inspect-panel-body">
                <ViewerPane
                  nodeKey={controller.selectedNode}
                  content={controller.inspect?.viewer_content}
                  editable={controller.inspect?.edit.editable}
                  versionToken={controller.inspect?.edit.version_token}
                  lastSavedBackupAvailable={controller.inspect?.edit.last_saved_backup_available}
                  onSave={controller.saveInspectContent}
                  onReload={controller.reloadInspectNode}
                  onRevert={controller.revertInspectSave}
                />
              </div>
            </div>

            <div className="panel inspect-panel inspect-panel-reasons">
              <h2>Reasons</h2>
              <div className="inspect-panel-body">
                <InspectReasonsPane
                  currentNode={controller.currentNode}
                  inspect={controller.inspect}
                />
              </div>
            </div>
          </section>
        )}

        {controller.activeTab === "Activity" && (
          <section className="panel">
            <div className="activity-header">
              <h2>Activity</h2>
              <button onClick={() => void controller.refreshActivity()}>Refresh observed</button>
            </div>
            <ul>
              {controller.inspect?.related_activity.map((item) => (
                <li key={item.payload_ref}>
                  {item.payload_ref} ({item.confidence.toFixed(2)})
                </li>
              ))}
            </ul>
          </section>
        )}
      </main>
    </div>
  );
}
