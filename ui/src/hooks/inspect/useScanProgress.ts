import { useEffect, useRef, useState, useMemo } from "react";
import type { JobStatus } from "../../lib/types";
import { apiUrl, parseApiError, SCAN_STATUS_LINGER_MS } from "./util";

interface UseScanProgressProps {
  apiBase: string;
  selectedProject: string;
  selectedTool: string;
  setStatusMessage: (msg: string) => void;
  loadProjects: () => Promise<any>;
}

export function useScanProgress({
  apiBase,
  selectedProject,
  selectedTool,
  setStatusMessage,
  loadProjects,
}: UseScanProgressProps) {
  const [scanJob, setScanJob] = useState<JobStatus | null>(null);
  const [isStartingGlobalScan, setIsStartingGlobalScan] = useState(false);
  const [isStartingScopedReindex, setIsStartingScopedReindex] = useState(false);
  const clearScanJobTimer = useRef<number | null>(null);
  const scanJobPollTimer = useRef<number | null>(null);

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
    if (!scanJob || scanJob.status === "running") {
      return;
    }
    stopScanJobPolling();
    if (scanJob.scope_kind === "global") {
      loadProjects().catch((error) => setStatusMessage(String(error)));
      return;
    }
  }, [scanJob, loadProjects, setStatusMessage]);

  const scopedScanJob = useMemo(() => 
    scanJob?.kind === "scan" &&
    scanJob.scope_kind === "project_tool" &&
    scanJob.project_id === selectedProject &&
    scanJob.tool === selectedTool
      ? scanJob
      : null,
    [scanJob, selectedProject, selectedTool]
  );

  const globalScanJob = useMemo(() => 
    scanJob?.kind === "scan" && scanJob.scope_kind === "global" ? scanJob : null,
    [scanJob]
  );

  const isGlobalScanRunning = isStartingGlobalScan || (globalScanJob?.status === "running" && globalScanJob.kind === "scan");
  const isScopedReindexRunning = isStartingScopedReindex || (scopedScanJob?.status === "running" && scopedScanJob.kind === "scan");

  return {
    scanJob,
    setScanJob,
    runGlobalScan,
    runScopedReindex,
    isGlobalScanRunning,
    isScopedReindexRunning,
    globalScanJob,
    scopedScanJob,
  };
}
