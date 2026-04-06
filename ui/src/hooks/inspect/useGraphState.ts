import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { JobStatus, SurfaceState } from "../../lib/types";
import {
  buildInspectTree,
  collectAllDirectoryKeys,
  collectSelectedAncestorKeys,
  pickNextSelectedNode,
} from "../../lib/inspect";
import { apiUrl, nodeStorageKey, treeStorageKey } from "./util";

interface UseGraphStateProps {
  apiBase: string;
  selectedProject: string;
  selectedTool: string;
  setStatusMessage: (msg: string) => void;
  scanJob: JobStatus | null;
  docUrl: string;
}

export function useGraphState({
  apiBase,
  selectedProject,
  selectedTool,
  setStatusMessage,
  scanJob,
  docUrl,
}: UseGraphStateProps) {
  const [graph, setGraph] = useState<SurfaceState | null>(null);
  const [selectedNode, setSelectedNode] = useState<string>("");
  const [expandedTreeKeys, setExpandedTreeKeys] = useState<string[]>([]);
  const [hasStoredExpandedTreeState, setHasStoredExpandedTreeState] = useState(false);
  const lastAutoExpandedSelection = useRef("");

  async function refreshGraph(projectId = selectedProject, toolId = selectedTool) {
    if (!projectId) {
      setGraph(null);
      return null;
    }
    try {
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
    } catch (error) {
      setStatusMessage(String(error));
      throw error;
    }
  }

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
    refreshGraph().catch((error) => setStatusMessage(String(error)));
  }, [apiBase, selectedProject, selectedTool]);

  useEffect(() => {
    if (!scanJob || scanJob.status === "running") {
      return;
    }
    if (
      scanJob.scope_kind === "project_tool" &&
      scanJob.project_id === selectedProject &&
      scanJob.tool === selectedTool
    ) {
      refreshGraph(selectedProject, selectedTool).catch((error) => setStatusMessage(String(error)));
    }
  }, [scanJob, selectedProject, selectedTool, setStatusMessage]);

  const tree = useMemo(() => buildInspectTree(graph), [graph]);
  const allTreeKeys = useMemo(() => collectAllDirectoryKeys(tree), [tree]);
  const selectedAncestorKeys = useMemo(
    () => collectSelectedAncestorKeys(tree, selectedNode),
    [selectedNode, tree],
  );
  const currentNode = useMemo(
    () => graph?.nodes.find((node) => node.id === selectedNode) ?? null,
    [graph, selectedNode],
  );

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
    try {
      await fetch(apiUrl(apiBase, "/api/docs/fetch"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ url: docUrl, project_id: selectedProject, tool: selectedTool }),
      });
      await refreshGraph(selectedProject, selectedTool);
      setStatusMessage("Docs snapshot saved.");
    } catch (error) {
      setStatusMessage(String(error));
    }
  }

  const toggleExpandedKey = useCallback((key: string) => {
    setHasStoredExpandedTreeState(true);
    setExpandedTreeKeys((current) =>
      current.includes(key) ? current.filter((entry) => entry !== key) : [...current, key],
    );
  }, []);

  const expandAllTree = useCallback(() => {
    setHasStoredExpandedTreeState(true);
    setExpandedTreeKeys(allTreeKeys);
  }, [allTreeKeys]);

  const collapseAllTree = useCallback(() => {
    setHasStoredExpandedTreeState(true);
    setExpandedTreeKeys([]);
  }, []);

  return {
    graph,
    setGraph,
    selectedNode,
    setSelectedNode,
    expandedTreeKeys,
    setExpandedTreeKeys,
    refreshGraph,
    tree,
    allTreeKeys,
    currentNode,
    fetchDocs,
    toggleExpandedKey,
    expandAllTree,
    collapseAllTree,
  };
}
