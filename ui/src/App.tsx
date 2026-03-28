import { useEffect, useMemo, useState } from "react";

type ProjectSummary = {
  id: string;
  root_path: string;
  name: string;
  indexed_at: string;
  status: string;
};

type ToolContext = {
  id: string;
  display_name: string;
  support_level: string;
};

type SurfaceState = {
  project: ProjectSummary;
  tool: ToolContext;
  nodes: Array<{ id: string; kind: string; [key: string]: unknown }>;
  edges: Array<{ from: string; to: string; edge_type: string; reason: string }>;
  verdicts: Array<{
    entity_id: string;
    states: string[];
    why_included: string[];
    why_excluded: string[];
  }>;
};

type InspectPayload = {
  entity: { id: string; kind: string; [key: string]: unknown };
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

const TOOLS = [
  "claude_code",
  "claude_cowork",
  "codex",
  "codex_cli",
  "copilot_cli",
  "intellij_copilot",
  "opencode",
  "antigravity",
];

const LABELS: Record<string, string> = {
  claude_code: "Claude Code",
  claude_cowork: "Claude Cowork",
  codex: "Codex",
  codex_cli: "Codex CLI",
  copilot_cli: "Copilot CLI",
  intellij_copilot: "IntelliJ/Copilot",
  opencode: "OpenCode",
  antigravity: "Antigravity",
};

const TABS = ["Projects", "Docs", "Tool", "Inspect", "Activity"] as const;
const BUILD_CHECK_MS = 180_000;
const CURRENT_BUILD_ID = import.meta.env.VITE_BUILD_ID ?? "dev";
const BUILD_META_PATH = `${import.meta.env.BASE_URL}build-meta.json`;

function defaultApiBase() {
  const params = new URLSearchParams(window.location.search);
  const fromQuery = params.get("apiBase");
  if (fromQuery) return fromQuery;

  const fromStorage = window.localStorage.getItem("harnessInspector.apiBase");
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

export function App() {
  const [activeTab, setActiveTab] = useState<(typeof TABS)[number]>("Projects");
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [selectedProject, setSelectedProject] = useState<string>("");
  const [selectedTool, setSelectedTool] = useState<string>("codex");
  const [graph, setGraph] = useState<SurfaceState | null>(null);
  const [selectedNode, setSelectedNode] = useState<string>("");
  const [inspect, setInspect] = useState<InspectPayload | null>(null);
  const [docUrl, setDocUrl] = useState("https://developers.openai.com/codex/plugins");
  const [statusMessage, setStatusMessage] = useState("Ready.");
  const [apiBase, setApiBase] = useState(defaultApiBase);
  const [staleBuildMessage, setStaleBuildMessage] = useState("");

  async function loadProjects() {
    const response = await fetch(apiUrl(apiBase, "/api/projects"));
    const payload = (await response.json()) as ProjectSummary[];
    setProjects(payload);
    if (!selectedProject && payload[0]) {
      setSelectedProject(payload[0].id);
    }
  }

  async function runScan() {
    setStatusMessage("Scanning local roots.");
    await fetch(apiUrl(apiBase, "/api/scan"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    await loadProjects();
    setStatusMessage("Scan complete.");
  }

  useEffect(() => {
    window.localStorage.setItem("harnessInspector.apiBase", apiBase);
  }, [apiBase]);

  useEffect(() => {
    if (CURRENT_BUILD_ID === "dev") {
      return;
    }

    let lastCheckAt = 0;

    const checkForNewBuild = async () => {
      lastCheckAt = Date.now();
      try {
        const response = await fetch(`${BUILD_META_PATH}?ts=${Date.now()}`, {
          cache: "no-store",
        });
        if (!response.ok) return;
        const payload = (await response.json()) as { buildId?: string };
        if (payload.buildId && payload.buildId !== CURRENT_BUILD_ID) {
          setStaleBuildMessage("New deploy detected. Reloading.");
          const url = new URL(window.location.href);
          url.searchParams.set("build", payload.buildId);
          window.location.replace(url.toString());
        }
      } catch {
        // no-op; stale check should never block usage
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
    if (!selectedProject) return;
    fetch(apiUrl(apiBase, `/api/projects/${selectedProject}/graph?tool=${selectedTool}`))
      .then((response) => response.json())
      .then((payload: SurfaceState) => {
        setGraph(payload);
        const preferred = payload.nodes.find((node) => node.kind !== "tool_context");
        setSelectedNode(preferred?.id ?? "");
      })
      .catch((error) => setStatusMessage(String(error)));
  }, [apiBase, selectedProject, selectedTool]);

  useEffect(() => {
    if (!selectedProject || !selectedNode) return;
    fetch(
      apiUrl(
        apiBase,
        `/api/projects/${selectedProject}/inspect?tool=${selectedTool}&node=${encodeURIComponent(selectedNode)}`,
      ),
    )
      .then((response) => response.json())
      .then((payload: InspectPayload) => setInspect(payload))
      .catch((error) => setStatusMessage(String(error)));
  }, [apiBase, selectedProject, selectedNode, selectedTool]);

  const prioritizedNodes = useMemo(() => {
    if (!graph) return [];
    const score = (node: { id: string }) => {
      const verdict = graph.verdicts.find((item) => item.entity_id === node.id);
      const states = verdict?.states ?? [];
      if (states.includes("effective")) return 0;
      if (states.includes("misleading")) return 1;
      if (states.includes("referenced_only")) return 2;
      return 3;
    };
    return [...graph.nodes].sort((left, right) => score(left) - score(right));
  }, [graph]);

  async function fetchDocs() {
    if (!selectedProject) return;
    setStatusMessage("Fetching docs snapshot.");
    await fetch(apiUrl(apiBase, "/api/docs/fetch"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url: docUrl, project_id: selectedProject, tool: selectedTool }),
    });
    setStatusMessage("Docs snapshot saved.");
    const response = await fetch(
      apiUrl(apiBase, `/api/projects/${selectedProject}/graph?tool=${selectedTool}`),
    );
    setGraph((await response.json()) as SurfaceState);
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
      setInspect((await response.json()) as InspectPayload);
    }
    setStatusMessage("Activity refresh complete.");
  }

  return (
    <div className="shell">
      <aside className="nav">
        <div className="brand">
          <p>Harness Inspector</p>
          <span>Truth over elegance.</span>
        </div>
        {TABS.map((tab) => (
          <button
            key={tab}
            className={tab === activeTab ? "tab active" : "tab"}
            onClick={() => setActiveTab(tab)}
          >
            {tab}
          </button>
        ))}
        <button className="scan" onClick={runScan}>
          Reindex
        </button>
        <p className="status">{statusMessage}</p>
      </aside>

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
              {TOOLS.map((tool) => (
                <option key={tool} value={tool}>
                  {LABELS[tool]}
                </option>
              ))}
            </select>
          </label>
          <label className="api-field">
            API Base
            <input
              value={apiBase}
              onChange={(event) => setApiBase(event.target.value)}
              placeholder="http://127.0.0.1:8765"
            />
          </label>
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
                  <span>{project.root_path}</span>
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
              <button onClick={fetchDocs}>Fetch snapshot</button>
            </div>
            <p>Snapshot binds to selected project and tool context.</p>
          </section>
        )}

        {activeTab === "Tool" && (
          <section className="panel">
            <h2>Tool Context</h2>
            <p>{LABELS[selectedTool]}</p>
            <p>Tree-first inspect. Graph as source of truth.</p>
          </section>
        )}

        {activeTab === "Inspect" && (
          <section className="inspect-grid">
            <div className="panel left">
              <h2>Effective Context Tree</h2>
              <div className="node-list">
                {prioritizedNodes.map((node) => (
                  <button
                    key={node.id}
                    className={node.id === selectedNode ? "node active" : "node"}
                    onClick={() => setSelectedNode(node.id)}
                  >
                    <strong>{String((node as Record<string, unknown>).name ?? (node as Record<string, unknown>).path ?? node.id)}</strong>
                    <span>{node.kind}</span>
                  </button>
                ))}
              </div>
            </div>
            <div className="panel center">
              <h2>Viewer</h2>
              <pre>{inspect?.viewer_content ?? "Select a node."}</pre>
            </div>
            <div className="panel right">
              <h2>Reasons</h2>
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
              <button onClick={refreshActivity}>Refresh observed</button>
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
    </div>
  );
}
