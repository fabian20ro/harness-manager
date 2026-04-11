import { useEffect, useState } from "react";
import type { InspectPayload, SaveInspectResponse, SurfaceState } from "../../lib/types";
import { apiUrl, formatInspectFailureMessage } from "./util";

interface UseInspectContentProps {
  apiBase: string;
  selectedProject: string;
  selectedTool: string;
  selectedNode: string;
  graph: SurfaceState | null;
  setGraph: (graph: SurfaceState | null) => void;
  setStatusMessage: (msg: string) => void;
}

export function useInspectContent({
  apiBase,
  selectedProject,
  selectedTool,
  selectedNode,
  graph,
  setGraph,
  setStatusMessage,
}: UseInspectContentProps) {
  const [inspect, setInspect] = useState<InspectPayload | null>(null);
  const [inspectStatusMessage, setInspectStatusMessage] = useState("");

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

  async function reloadInspectNode() {
    if (!selectedProject || !selectedNode) return;
    try {
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
    } catch (error) {
      setStatusMessage(String(error));
    }
  }

  async function saveInspectContent(content: string, versionToken: string) {
    if (!selectedProject || !selectedNode) return;
    try {
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
    } catch (error) {
      setStatusMessage(String(error));
      throw error;
    }
  }

  async function revertInspectSave() {
    if (!selectedProject || !selectedNode) return;
    try {
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
    } catch (error) {
      setStatusMessage(String(error));
      throw error;
    }
  }

  async function fixInspectCheck(checkLabel: string) {
    if (!selectedProject || !selectedNode) return;
    try {
      const response = await fetch(
        apiUrl(apiBase, `/api/projects/${selectedProject}/inspect/fix`),
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            tool: selectedTool,
            node: selectedNode,
            check_label: checkLabel,
          }),
        },
      );
      if (!response.ok) {
        const payload = (await response.json()) as { error?: string };
        throw new Error(payload.error ?? `Fix failed: ${response.status}`);
      }
      const payload = (await response.json()) as SaveInspectResponse;
      setGraph(payload.graph);
      setInspect(payload.inspect);
      setStatusMessage(payload.status_message);
    } catch (error) {
      setStatusMessage(String(error));
      throw error;
    }
  }

  async function refreshActivity() {
    if (!selectedProject) return;
    setStatusMessage("Refreshing observed activity.");
    try {
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
    } catch (error) {
      setStatusMessage(String(error));
    }
  }

  return {
    inspect,
    setInspect,
    inspectStatusMessage,
    reloadInspectNode,
    saveInspectContent,
    revertInspectSave,
    fixInspectCheck,
    refreshActivity,
  };
}
