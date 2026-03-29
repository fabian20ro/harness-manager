import { useEffect, useMemo, useRef, useState } from "react";

import {
  buildInspectTree,
  collectAllDirectoryKeys,
  collectSelectedAncestorKeys,
  type AppTab,
  HELPER_COMMAND,
  LABELS,
  pickNextSelectedNode,
  TOOL_IDS,
} from "../lib/inspect";
import type {
  GraphNodeRecord,
  InspectPayload,
  JobStatus,
  ProjectSummary,
  SaveInspectResponse,
  SurfaceState,
} from "../lib/types";

const BUILD_CHECK_MS = 180_000;
const SCAN_STATUS_LINGER_MS = 4_000;
const CURRENT_BUILD_ID = import.meta.env.VITE_BUILD_ID ?? "dev";
const BUILD_META_PATH = `${import.meta.env.BASE_URL}build-meta.json`;
const STORAGE_PREFIX = "harnessInspector";

function defaultApiBase() {
  const params = new URLSearchParams(window.location.search);
  const fromQuery = params.get("apiBase");
  if (fromQuery) return fromQuery;

  const fromStorage = window.localStorage.getItem(`${STORAGE_PREFIX}.apiBase`);
  if (fromStorage) return fromStorage;

  const host = window.location.hostname;
  if (host === "127.0.0.1" || host === "localhost") {
    return "";
  }
  if (host.endsWith("github.io")) {
    return "http://127.0.0.1:8765";
  }
  return "";
}

function apiUrl(apiBase: string, path: string) {
  return apiBase ? `${apiBase}${path}` : path;
}

function loadStored<T>(key: string, fallback: T) {
  const raw = window.localStorage.getItem(key);
  return raw ? (JSON.parse(raw) as T) : fallback;
}

function nodeStorageKey(projectId: string, toolId: string) {
  return `${STORAGE_PREFIX}.inspectNode.${projectId}.${toolId}`;
}

function treeStorageKey(projectId: string, toolId: string) {
  return `${STORAGE_PREFIX}.inspectTreeExpanded.${projectId}.${toolId}`;
}

function formatInspectFailureMessage(
  nodeLabel: string,
  status: number,
  payload?: { error?: string } | null,
) {
  const detail = payload?.error?.trim() || `HTTP ${status}`;
  return `Inspect failed for ${nodeLabel}: ${detail}`;
}

async function parseApiError(response: Response, fallback: string) {
  const payload = (await response.json().catch(() => null)) as { error?: string } | null;
  return payload?.error?.trim() || fallback;
}

export function useInspectController() {
  const [activeTab, setActiveTab] = useState<AppTab>(
    () => loadStored(`${STORAGE_PREFIX}.activeTab`, "Projects") as AppTab,
  );
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [selectedProject, setSelectedProject] = useState<string>(
    () => window.localStorage.getItem(`${STORAGE_PREFIX}.selectedProject`) ?? "",
  );
  const [selectedTool, setSelectedTool] = useState<string>(
    () => window.localStorage.getItem(`${STORAGE_PREFIX}.selectedTool`) ?? "codex",
  );
  const [sidebarCollapsed, setSidebarCollapsed] = useState<boolean>(
    () => loadStored(`${STORAGE_PREFIX}.sidebarCollapsed`, false),
  );
  const [graph, setGraph] = useState<SurfaceState | null>(null);
  const [selectedNode, setSelectedNode] = useState<string>("");
  const [inspect, setInspect] = useState<InspectPayload | null>(null);
  const [docUrl, setDocUrl] = useState("https://developers.openai.com/codex/skills");
  const [statusMessage, setStatusMessage] = useState("");
  const [inspectStatusMessage, setInspectStatusMessage] = useState("");
  const [scanJob, setScanJob] = useState<JobStatus | null>(null);
  const [apiBase, setApiBase] = useState(defaultApiBase);
  const [staleBuildMessage, setStaleBuildMessage] = useState("");
  const [expandedTreeKeys, setExpandedTreeKeys] = useState<string[]>([]);
  const [hasStoredExpandedTreeState, setHasStoredExpandedTreeState] = useState(false);
  const [isStartingGlobalScan, setIsStartingGlobalScan] = useState(false);
  const [isStartingScopedReindex, setIsStartingScopedReindex] = useState(false);
  const clearScanJobTimer = useRef<number | null>(null);
  const scanJobPollTimer = useRef<number | null>(null);
  const lastAutoExpandedSelection = useRef("");

  function stopScanJobPolling() {
    if (scanJobPollTimer.current) {
      window.clearInterval(scanJobPollTimer.current);
      scanJobPollTimer.current = null;
    }
  }

  function startScanJobPolling(jobId: string) {
    stopScanJobPolling();
    scanJobPollTimer.current = window.setInterval(async () => {
      try {
        const response = await fetch(apiUrl(apiBase, `/api/jobs/${jobId}`));
        if (!response.ok) {
          return;
        }
        const payload = (await response.json()) as JobStatus;
        setScanJob((current) => (current?.id === jobId || !current ? payload : current));
        if (payload.status !== "running") {
          stopScanJobPolling();
        }
      } catch {
        // Keep SSE as the primary channel. Polling is fallback-only.
      }
    }, 750);
  }

  async function loadProjects() {
    const response = await fetch(apiUrl(apiBase, "/api/projects"));
    const payload = (await response.json()) as ProjectSummary[];
    setProjects(payload);
    setSelectedProject((current) => {
      if (current && payload.some((project) => project.id === current)) {
        return current;
      }
      return payload[0]?.id ?? "";
    });
    return payload;
  }

  async function refreshGraph(projectId = selectedProject, toolId = selectedTool) {
    if (!projectId) {
      setGraph(null);
      return null;
    }
    const response = await fetch(apiUrl(apiBase, `/api/projects/${projectId}/graph?tool=${toolId}`));
    if (!response.ok) {
      throw new Error(`Graph failed: ${response.status}`);
    }
    const payload = (await response.json()) as SurfaceState;
    setGraph(payload);
    setSelectedNode((current) => {
      const stored = window.localStorage.getItem(nodeStorageKey(projectId, toolId));
      return pickNextSelectedNode(stored || current, payload.nodes, payload.verdicts);
    });
    return payload;
  }

  async function runGlobalScan() {
    setIsStartingGlobalScan(true);
    try {
      const response = await fetch(apiUrl(apiBase, "/api/scan"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({}),
      });
      if (!response.ok) {
        throw new Error(await parseApiError(response, `Scan failed: ${response.status}`));
      }
      const payload = (await response.json()) as JobStatus;
      setScanJob(payload);
      startScanJobPolling(payload.id);
    } catch (error) {
      setStatusMessage(String(error));
      throw error;
    } finally {
      setIsStartingGlobalScan(false);
    }
  }

  async function runScopedReindex() {
    if (!selectedProject) return;
    setIsStartingScopedReindex(true);
    try {
      const response = await fetch(apiUrl(apiBase, `/api/projects/${selectedProject}/reindex`), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ tool: selectedTool }),
      });
      if (!response.ok) {
        throw new Error(
          await parseApiError(response, `Scoped reindex failed: ${response.status}`),
        );
      }
      const payload = (await response.json()) as JobStatus;
      setScanJob(payload);
      startScanJobPolling(payload.id);
    } catch (error) {
      setStatusMessage(String(error));
      throw error;
    } finally {
      setIsStartingScopedReindex(false);
    }
  }

  async function copyHelperCommand() {
    try {
      await navigator.clipboard.writeText(HELPER_COMMAND);
      setStatusMessage(`Copied ${HELPER_COMMAND}.`);
    } catch (error) {
      setStatusMessage(`Clipboard copy failed: ${String(error)}`);
    }
  }

  useEffect(() => {
    window.localStorage.setItem(`${STORAGE_PREFIX}.apiBase`, apiBase);
  }, [apiBase]);

  useEffect(() => {
    window.localStorage.setItem(`${STORAGE_PREFIX}.activeTab`, JSON.stringify(activeTab));
  }, [activeTab]);

  useEffect(() => {
    window.localStorage.setItem(`${STORAGE_PREFIX}.selectedProject`, selectedProject);
  }, [selectedProject]);

  useEffect(() => {
    window.localStorage.setItem(`${STORAGE_PREFIX}.selectedTool`, selectedTool);
  }, [selectedTool]);

  useEffect(() => {
    window.localStorage.setItem(
      `${STORAGE_PREFIX}.sidebarCollapsed`,
      JSON.stringify(sidebarCollapsed),
    );
  }, [sidebarCollapsed]);

  useEffect(() => {
    if (selectedProject && selectedTool && selectedNode) {
      window.localStorage.setItem(nodeStorageKey(selectedProject, selectedTool), selectedNode);
    }
  }, [selectedNode, selectedProject, selectedTool]);

  useEffect(() => {
    if (!selectedProject) {
      setExpandedTreeKeys([]);
      setHasStoredExpandedTreeState(false);
      lastAutoExpandedSelection.current = "";
      return;
    }
    const stored = window.localStorage.getItem(treeStorageKey(selectedProject, selectedTool));
    setExpandedTreeKeys(stored ? (JSON.parse(stored) as string[]) : []);
    setHasStoredExpandedTreeState(Boolean(stored));
    lastAutoExpandedSelection.current = "";
  }, [selectedProject, selectedTool]);

  useEffect(() => {
    if (!selectedProject) return;
    window.localStorage.setItem(
      treeStorageKey(selectedProject, selectedTool),
      JSON.stringify(expandedTreeKeys),
    );
  }, [expandedTreeKeys, selectedProject, selectedTool]);

  useEffect(() => {
    if (CURRENT_BUILD_ID === "dev") {
      return;
    }

    let lastCheckAt = 0;
    const checkForNewBuild = async () => {
      lastCheckAt = Date.now();
      try {
        const response = await fetch(`${BUILD_META_PATH}?ts=${Date.now()}`, { cache: "no-store" });
        if (!response.ok) return;
        const payload = (await response.json()) as { buildId?: string };
        if (payload.buildId && payload.buildId !== CURRENT_BUILD_ID) {
          setStaleBuildMessage("New deploy detected. Reloading.");
          const url = new URL(window.location.href);
          url.searchParams.set("build", payload.buildId);
          window.location.replace(url.toString());
        }
      } catch {
        // ignore
      }
    };

    const interval = window.setInterval(checkForNewBuild, BUILD_CHECK_MS);
    const onVisible = () => {
      if (document.visibilityState === "visible" && Date.now() - lastCheckAt >= BUILD_CHECK_MS) {
        void checkForNewBuild();
      }
    };
    document.addEventListener("visibilitychange", onVisible);
    void checkForNewBuild();
    return () => {
      window.clearInterval(interval);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, []);

  useEffect(() => {
    loadProjects().catch((error) => setStatusMessage(String(error)));
  }, [apiBase]);

  useEffect(() => {
    if (typeof window.EventSource === "undefined") {
      return;
    }

    const source = new window.EventSource(apiUrl(apiBase, "/api/events"));
    source.onmessage = (event) => {
      const job = JSON.parse(event.data) as JobStatus;
      if (job.kind === "scan") {
        setScanJob(job);
        if (job.status !== "running") {
          stopScanJobPolling();
        }
        if (clearScanJobTimer.current) {
          window.clearTimeout(clearScanJobTimer.current);
          clearScanJobTimer.current = null;
        }
        if (job.status !== "running") {
          clearScanJobTimer.current = window.setTimeout(() => {
            setScanJob((current) => (current?.id === job.id ? null : current));
          }, SCAN_STATUS_LINGER_MS);
        }
        return;
      }
      if (job.status !== "running") {
        setStatusMessage(job.message);
      }
    };
    source.onerror = () => {
      setStatusMessage("Job event stream unavailable.");
    };

    return () => {
      source.close();
      stopScanJobPolling();
      if (clearScanJobTimer.current) {
        window.clearTimeout(clearScanJobTimer.current);
        clearScanJobTimer.current = null;
      }
    };
  }, [apiBase]);

  useEffect(() => {
    refreshGraph().catch((error) => setStatusMessage(String(error)));
  }, [apiBase, selectedProject, selectedTool]);

  useEffect(() => {
    if (!selectedProject || !selectedNode || !graph) {
      setInspect(null);
      setInspectStatusMessage("");
      return;
    }
    const selectedGraphNode = graph.nodes.find((node) => node.id === selectedNode);
    if (!selectedGraphNode) {
      setInspect(null);
      setInspectStatusMessage(`Inspect unavailable for ${selectedNode}: node not found.`);
      return;
    }

    setInspectStatusMessage("");

    fetch(
      apiUrl(
        apiBase,
        `/api/projects/${selectedProject}/inspect?tool=${selectedTool}&node=${encodeURIComponent(selectedNode)}`,
      ),
    )
      .then(async (response) => {
        if (!response.ok) {
          const payload = (await response.json().catch(() => null)) as { error?: string } | null;
          throw new Error(
            formatInspectFailureMessage(
              String(selectedGraphNode.display_path ?? selectedGraphNode.path ?? selectedNode),
              response.status,
              payload,
            ),
          );
        }
        return (await response.json()) as InspectPayload;
      })
      .then((payload) => {
        setInspect(payload);
        setInspectStatusMessage("");
      })
      .catch((error) => {
        setInspect(null);
        setInspectStatusMessage(String(error));
      });
  }, [apiBase, graph, selectedProject, selectedNode, selectedTool]);

  useEffect(() => {
    if (!scanJob || scanJob.status === "running") {
      return;
    }
    stopScanJobPolling();
    if (scanJob.scope_kind === "global") {
      loadProjects().catch((error) => setStatusMessage(String(error)));
      return;
    }
    if (
      scanJob.scope_kind === "project_tool" &&
      scanJob.project_id === selectedProject &&
      scanJob.tool === selectedTool
    ) {
      refreshGraph(selectedProject, selectedTool).catch((error) => setStatusMessage(String(error)));
    }
  }, [apiBase, scanJob, selectedProject, selectedTool]);

  const tree = useMemo(() => buildInspectTree(graph), [graph]);
  const allTreeKeys = useMemo(() => collectAllDirectoryKeys(tree), [tree]);
  const selectedAncestorKeys = useMemo(
    () => collectSelectedAncestorKeys(tree, selectedNode),
    [selectedNode, tree],
  );
  const selectedProjectMeta = useMemo(
    () => projects.find((project) => project.id === selectedProject),
    [projects, selectedProject],
  );
  const currentNode = useMemo(
    () => graph?.nodes.find((node) => node.id === selectedNode) ?? null,
    [graph, selectedNode],
  );
  const scopedScanJob =
    scanJob?.kind === "scan" &&
    scanJob.scope_kind === "project_tool" &&
    scanJob.project_id === selectedProject &&
    scanJob.tool === selectedTool
      ? scanJob
      : null;
  const globalScanJob =
    scanJob?.kind === "scan" && scanJob.scope_kind === "global" ? scanJob : null;

  useEffect(() => {
    if (!selectedProject || hasStoredExpandedTreeState || !tree.length) {
      return;
    }
    setExpandedTreeKeys(collectAllDirectoryKeys(tree));
    setHasStoredExpandedTreeState(true);
  }, [hasStoredExpandedTreeState, selectedProject, tree]);

  useEffect(() => {
    if (!selectedProject || !selectedNode || !tree.length) {
      return;
    }

    const selectionScope = `${selectedProject}:${selectedTool}:${selectedNode}`;
    if (lastAutoExpandedSelection.current === selectionScope) {
      return;
    }
    lastAutoExpandedSelection.current = selectionScope;

    setExpandedTreeKeys((current) => {
      const missing = selectedAncestorKeys.filter((key) => !current.includes(key));
      if (!missing.length) {
        return current;
      }
      return [...current, ...missing];
    });
  }, [selectedAncestorKeys, selectedNode, selectedProject, selectedTool, tree]);

  async function fetchDocs() {
    if (!selectedProject) return;
    setStatusMessage("Fetching docs snapshot.");
    await fetch(apiUrl(apiBase, "/api/docs/fetch"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url: docUrl, project_id: selectedProject, tool: selectedTool }),
    });
    await refreshGraph(selectedProject, selectedTool);
    setStatusMessage("Docs snapshot saved.");
  }

  async function refreshActivity() {
    if (!selectedProject) return;
    setStatusMessage("Refreshing observed activity.");
    await fetch(apiUrl(apiBase, "/api/activity/refresh"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ project_id: selectedProject, tool: selectedTool }),
    });
    if (selectedNode) {
      const response = await fetch(
        apiUrl(
          apiBase,
          `/api/projects/${selectedProject}/inspect?tool=${selectedTool}&node=${encodeURIComponent(selectedNode)}`,
        ),
      );
      if (response.ok) {
        setInspect((await response.json()) as InspectPayload);
      }
    }
    setStatusMessage("Activity refresh complete.");
  }

  async function reloadInspectNode() {
    if (!selectedProject || !selectedNode) return;
    const response = await fetch(
      apiUrl(
        apiBase,
        `/api/projects/${selectedProject}/inspect?tool=${selectedTool}&node=${encodeURIComponent(selectedNode)}`,
      ),
    );
    if (!response.ok) {
      throw new Error(`Reload failed: ${response.status}`);
    }
    setInspect((await response.json()) as InspectPayload);
    setStatusMessage("Reloaded from disk.");
  }

  async function saveInspectContent(content: string, versionToken: string) {
    if (!selectedProject || !selectedNode) return;
    const response = await fetch(apiUrl(apiBase, `/api/projects/${selectedProject}/inspect/save`), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        tool: selectedTool,
        node: selectedNode,
        content,
        version_token: versionToken,
      }),
    });
    if (!response.ok) {
      const payload = (await response.json()) as { error?: string };
      throw new Error(payload.error ?? `Save failed: ${response.status}`);
    }
    const payload = (await response.json()) as SaveInspectResponse;
    setGraph(payload.graph);
    setInspect(payload.inspect);
    setStatusMessage(payload.status_message);
  }

  async function revertInspectSave() {
    if (!selectedProject || !selectedNode) return;
    const response = await fetch(
      apiUrl(apiBase, `/api/projects/${selectedProject}/inspect/revert-last-save`),
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          tool: selectedTool,
          node: selectedNode,
        }),
      },
    );
    if (!response.ok) {
      const payload = (await response.json()) as { error?: string };
      throw new Error(payload.error ?? `Revert failed: ${response.status}`);
    }
    const payload = (await response.json()) as SaveInspectResponse;
    setGraph(payload.graph);
    setInspect(payload.inspect);
    setStatusMessage(payload.status_message);
  }

  function toggleExpandedKey(key: string) {
    setHasStoredExpandedTreeState(true);
    setExpandedTreeKeys((current) =>
      current.includes(key) ? current.filter((entry) => entry !== key) : [...current, key],
    );
  }

  function expandAllTree() {
    setHasStoredExpandedTreeState(true);
    setExpandedTreeKeys(allTreeKeys);
  }

  function collapseAllTree() {
    setHasStoredExpandedTreeState(true);
    setExpandedTreeKeys([]);
  }

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
    isGlobalScanRunning:
      isStartingGlobalScan || (globalScanJob?.status === "running" && globalScanJob.kind === "scan"),
    isScopedReindexRunning:
      isStartingScopedReindex ||
      (scopedScanJob?.status === "running" && scopedScanJob.kind === "scan"),
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
