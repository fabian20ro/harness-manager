import { useAppConfig } from "./inspect/useAppConfig";
import { useProjectState } from "./inspect/useProjectState";
import { useScanProgress } from "./inspect/useScanProgress";
import { useGraphState } from "./inspect/useGraphState";
import { useInspectContent } from "./inspect/useInspectContent";

export function useInspectController() {
  const {
    activeTab,
    setActiveTab,
    sidebarCollapsed,
    setSidebarCollapsed,
    apiBase,
    setApiBase,
    selectedTool,
    setSelectedTool,
    staleBuildMessage,
    statusMessage,
    setStatusMessage,
    docUrl,
    setDocUrl,
    copyHelperCommand,
  } = useAppConfig();

  const {
    projects,
    selectedProject,
    setSelectedProject,
    selectedProjectMeta,
    loadProjects,
  } = useProjectState({ apiBase, setStatusMessage });

  const {
    scanJob,
    runGlobalScan,
    runScopedReindex,
    isGlobalScanRunning,
    isScopedReindexRunning,
    globalScanJob,
    scopedScanJob,
  } = useScanProgress({
    apiBase,
    selectedProject,
    selectedTool,
    setStatusMessage,
    loadProjects,
  });

  const {
    graph,
    setGraph,
    selectedNode,
    setSelectedNode,
    expandedTreeKeys,
    refreshGraph,
    tree,
    allTreeKeys,
    currentNode,
    fetchDocs,
    toggleExpandedKey,
    expandAllTree,
    collapseAllTree,
  } = useGraphState({
    apiBase,
    selectedProject,
    selectedTool,
    setStatusMessage,
    scanJob,
    docUrl,
  });

  const {
    inspect,
    inspectStatusMessage,
    reloadInspectNode,
    saveInspectContent,
    revertInspectSave,
    refreshActivity,
  } = useInspectContent({
    apiBase,
    selectedProject,
    selectedTool,
    selectedNode,
    graph,
    setGraph,
    setStatusMessage,
  });

  return {
    activeTab,
    allTreeKeys,
    apiBase,
    currentNode,
    docUrl,
    collapseAllTree,
    fetchDocs,
    globalScanJob,
    graph,
    inspectStatusMessage,
    inspect,
    isGlobalScanRunning,
    isScopedReindexRunning,
    projects,
    reloadInspectNode,
    refreshActivity,
    revertInspectSave,
    runGlobalScan,
    runScopedReindex,
    saveInspectContent,
    scanJob,
    scopedScanJob,
    selectedNode,
    selectedProject,
    selectedProjectMeta,
    selectedTool,
    setActiveTab,
    setApiBase,
    setDocUrl,
    setSelectedNode,
    setSelectedProject,
    setSelectedTool,
    setSidebarCollapsed,
    sidebarCollapsed,
    staleBuildMessage,
    statusMessage,
    expandAllTree,
    toggleExpandedKey,
    tree,
    treeExpandedKeys: expandedTreeKeys,
    copyHelperCommand,
  };
}
