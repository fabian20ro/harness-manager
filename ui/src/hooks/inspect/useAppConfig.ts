import { useEffect, useState } from "react";
import { type AppTab, HELPER_COMMAND } from "../../lib/inspect";
import { loadStored, STORAGE_PREFIX, BUILD_CHECK_MS, BUILD_META_PATH, CURRENT_BUILD_ID } from "./util";

export function useAppConfig() {
  const [activeTab, setActiveTab] = useState<AppTab>(
    () => loadStored(`${STORAGE_PREFIX}.activeTab`, "Projects") as AppTab,
  );
  const [sidebarCollapsed, setSidebarCollapsed] = useState<boolean>(
    () => loadStored(`${STORAGE_PREFIX}.sidebarCollapsed`, false),
  );
  const [apiBase, setApiBase] = useState(() => {
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
  });
  const [selectedTool, setSelectedTool] = useState<string>(
    () => window.localStorage.getItem(`${STORAGE_PREFIX}.selectedTool`) ?? "codex",
  );
  const [staleBuildMessage, setStaleBuildMessage] = useState("");
  const [statusMessage, setStatusMessage] = useState("");
  const [docUrl, setDocUrl] = useState("https://developers.openai.com/codex/skills");

  useEffect(() => {
    window.localStorage.setItem(`${STORAGE_PREFIX}.apiBase`, apiBase);
  }, [apiBase]);

  useEffect(() => {
    window.localStorage.setItem(`${STORAGE_PREFIX}.activeTab`, JSON.stringify(activeTab));
  }, [activeTab]);

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

  async function copyHelperCommand() {
    try {
      await navigator.clipboard.writeText(HELPER_COMMAND);
      setStatusMessage(`Copied ${HELPER_COMMAND}.`);
    } catch (error) {
      setStatusMessage(`Clipboard copy failed: ${String(error)}`);
    }
  }

  return {
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
  };
}
