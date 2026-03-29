import { useEffect, useMemo, useRef, useState } from "react";

import { HelperCommand } from "./components/HelperCommand";
import { InspectTree } from "./components/InspectTree";
import { ScanStatusBar } from "./components/ScanStatusBar";
import { SidebarNav } from "./components/SidebarNav";
import { ViewerPane } from "./components/ViewerPane";
import {
  buildInspectTree,
  type AppTab,
  formatDisplayPath,
  getNodeLabel,
  HELPER_COMMAND,
  LABELS,
  pickNextSelectedNode,
  TOOL_IDS,
} from "./lib/inspect";
import type {
  GraphNodeRecord,
  InspectPayload,
  JobStatus,
  ProjectSummary,
  SaveInspectResponse,
  SurfaceState,
} from "./lib/types";

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

export function App() {
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
  const [docUrl, setDocUrl] = useState("https://developers.openai.com/codex/plugins");
  const [statusMessage, setStatusMessage] = useState("Ready.");
  const [scanJob, setScanJob] = useState<JobStatus | null>(null);
  const [apiBase, setApiBase] = useState(defaultApiBase);
  const [staleBuildMessage, setStaleBuildMessage] = useState("");
  const clearScanJobTimer = useRef<number | null>(null);

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
  }

  async function runScan() {
    setStatusMessage("Starting reindex.");
    const response = await fetch(apiUrl(apiBase, "/api/scan"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    if (!response.ok) {
      throw new Error(`Scan failed: ${response.status}`);
    }
    const payload = (await response.json()) as JobStatus;
    setStatusMessage(payload.message);
    await loadProjects();
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
        setStatusMessage(job.message);
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
      if (clearScanJobTimer.current) {
        window.clearTimeout(clearScanJobTimer.current);
        clearScanJobTimer.current = null;
      }
    };
  }, [apiBase]);

  useEffect(() => {
    if (!selectedProject) {
      setGraph(null);
      return;
    }
    fetch(apiUrl(apiBase, `/api/projects/${selectedProject}/graph?tool=${selectedTool}`))
      .then((response) => response.json())
      .then((payload: SurfaceState) => {
        setGraph(payload);
        setSelectedNode((current) => {
          const stored = window.localStorage.getItem(nodeStorageKey(selectedProject, selectedTool));
          return pickNextSelectedNode(stored || current, payload.nodes, payload.verdicts);
        });
      })
      .catch((error) => setStatusMessage(String(error)));
  }, [apiBase, selectedProject, selectedTool]);

  useEffect(() => {
    if (!selectedProject || !selectedNode || !graph) {
      setInspect(null);
      return;
    }
    if (!graph.nodes.some((node) => node.id === selectedNode)) {
      return;
    }

    fetch(
      apiUrl(
        apiBase,
        `/api/projects/${selectedProject}/inspect?tool=${selectedTool}&node=${encodeURIComponent(selectedNode)}`,
      ),
    )
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(`Inspect failed: ${response.status}`);
        }
        return (await response.json()) as InspectPayload;
      })
      .then((payload) => setInspect(payload))
      .catch((error) => setStatusMessage(String(error)));
  }, [apiBase, graph, selectedProject, selectedNode, selectedTool]);

  const tree = useMemo(() => buildInspectTree(graph), [graph]);
  const selectedProjectMeta = projects.find((project) => project.id === selectedProject);
  const currentNode = useMemo(
    () => graph?.nodes.find((node) => node.id === selectedNode) ?? null,
    [graph, selectedNode],
  );

  async function fetchDocs() {
    if (!selectedProject) return;
    setStatusMessage("Fetching docs snapshot.");
    await fetch(apiUrl(apiBase, "/api/docs/fetch"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url: docUrl, project_id: selectedProject, tool: selectedTool }),
    });
    const response = await fetch(
      apiUrl(apiBase, `/api/projects/${selectedProject}/graph?tool=${selectedTool}`),
    );
    setGraph((await response.json()) as SurfaceState);
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
    const response = await fetch(
      apiUrl(apiBase, `/api/projects/${selectedProject}/inspect/save`),
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          tool: selectedTool,
          node: selectedNode,
          content,
          version_token: versionToken,
        }),
      },
    );
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

  return (
    <div className={sidebarCollapsed ? "shell shell-collapsed" : "shell"}>
      <SidebarNav
        activeTab={activeTab}
        collapsed={sidebarCollapsed}
        onSelectTab={setActiveTab}
        onToggleCollapse={() => setSidebarCollapsed((value) => !value)}
        onReindex={() => void runScan().catch((error) => setStatusMessage(String(error)))}
      />

      <main className="workspace">
        <header className="toolbar">
          <label>
            Project
            <select value={selectedProject} onChange={(event) => setSelectedProject(event.target.value)}>
              <option value="">Select project</option>
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Tool
            <select value={selectedTool} onChange={(event) => setSelectedTool(event.target.value)}>
              {TOOL_IDS.map((tool) => (
                <option key={tool} value={tool}>
                  {LABELS[tool]}
                </option>
              ))}
            </select>
          </label>
          <div className="toolbar-api-group">
            <label className="api-field">
              API Base
              <input
                value={apiBase}
                onChange={(event) => setApiBase(event.target.value)}
                placeholder="http://127.0.0.1:8765"
              />
            </label>
            <HelperCommand onCopy={() => void copyHelperCommand()} />
          </div>
        </header>

        {staleBuildMessage ? <p className="stale-banner">{staleBuildMessage}</p> : null}

        {activeTab === "Projects" && (
          <section className="panel">
            <h2>Projects</h2>
            <div className="project-list">
              {projects.map((project) => (
                <button
                  key={project.id}
                  className={project.id === selectedProject ? "project-card active" : "project-card"}
                  onClick={() => setSelectedProject(project.id)}
                >
                  <strong>{project.name}</strong>
                  <span>{formatDisplayPath(project.display_path)}</span>
                  <em>{new Date(project.indexed_at).toLocaleString()}</em>
                </button>
              ))}
            </div>
          </section>
        )}

        {activeTab === "Docs" && (
          <section className="panel">
            <h2>Docs</h2>
            <div className="docs-form">
              <input value={docUrl} onChange={(event) => setDocUrl(event.target.value)} />
              <button onClick={() => void fetchDocs()}>Fetch snapshot</button>
            </div>
            <p>Snapshot binds to selected project and tool context.</p>
          </section>
        )}

        {activeTab === "Tool" && (
          <section className="panel">
            <h2>Tool Context</h2>
            <p>{LABELS[selectedTool as keyof typeof LABELS] ?? selectedTool}</p>
            <p>Project: {selectedProjectMeta?.display_path ?? "No project selected"}</p>
          </section>
        )}

        {activeTab === "Inspect" && (
          <section className="inspect-grid">
            <div className="panel left inspect-panel">
              <h2>Effective Context Tree</h2>
              <InspectTree tree={tree} selectedNodeId={selectedNode} onSelect={setSelectedNode} />
            </div>
            <div className="panel center inspect-panel">
              <h2>{currentNode ? getNodeLabel(currentNode) : "Viewer"}</h2>
              <ViewerPane
                nodeKey={selectedNode}
                content={inspect?.viewer_content}
                editable={inspect?.edit.editable}
                versionToken={inspect?.edit.version_token}
                lastSavedBackupAvailable={inspect?.edit.last_saved_backup_available}
                onSave={saveInspectContent}
                onReload={reloadInspectNode}
                onRevert={revertInspectSave}
              />
            </div>
            <div className="panel right inspect-panel">
              <h2>Reasons</h2>
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
              <h3>Why in</h3>
              <ul>
                {inspect?.verdict?.why_included.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
              <h3>Why out</h3>
              <ul>
                {inspect?.verdict?.why_excluded.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
              <h3>References out</h3>
              <ul>
                {inspect?.outgoing_edges.map((edge) => (
                  <li key={`${edge.to}-${edge.edge_type}`}>
                    {edge.edge_type}: {edge.reason}
                  </li>
                ))}
              </ul>
            </div>
          </section>
        )}

        {activeTab === "Activity" && (
          <section className="panel">
            <div className="activity-header">
              <h2>Activity</h2>
              <button onClick={() => void refreshActivity()}>Refresh observed</button>
            </div>
            <ul>
              {inspect?.related_activity.map((item) => (
                <li key={item.payload_ref}>
                  {item.payload_ref} ({item.confidence.toFixed(2)})
                </li>
              ))}
            </ul>
          </section>
        )}
      </main>
      <ScanStatusBar job={scanJob} />
    </div>
  );
}
