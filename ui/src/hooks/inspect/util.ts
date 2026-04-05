export const BUILD_CHECK_MS = 180_000;
export const SCAN_STATUS_LINGER_MS = 4_000;
export const CURRENT_BUILD_ID = (import.meta as any).env?.VITE_BUILD_ID ?? "dev";
export const BUILD_META_PATH = `${(import.meta as any).env?.BASE_URL ?? "/"}build-meta.json`;
export const STORAGE_PREFIX = "harnessInspector";

export function defaultApiBase() {
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

export function apiUrl(apiBase: string, path: string) {
  return apiBase ? `${apiBase}${path}` : path;
}

export function loadStored<T>(key: string, fallback: T) {
  const raw = window.localStorage.getItem(key);
  return raw ? (JSON.parse(raw) as T) : fallback;
}

export function nodeStorageKey(projectId: string, toolId: string) {
  return `${STORAGE_PREFIX}.inspectNode.${projectId}.${toolId}`;
}

export function treeStorageKey(projectId: string, toolId: string) {
  return `${STORAGE_PREFIX}.inspectTreeExpanded.${projectId}.${toolId}`;
}

export function formatInspectFailureMessage(
  nodeLabel: string,
  status: number,
  payload?: { error?: string } | null,
) {
  const detail = payload?.error?.trim() || `HTTP ${status}`;
  return `Inspect failed for ${nodeLabel}: ${detail}`;
}

export async function parseApiError(response: Response, fallback: string) {
  const payload = (await response.json().catch(() => null)) as { error?: string } | null;
  return payload?.error?.trim() || fallback;
}
