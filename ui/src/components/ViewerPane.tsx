import { useEffect, useState } from "react";

type ViewerPaneProps = {
  content?: string;
  editable?: boolean;
  versionToken?: string;
  lastSavedBackupAvailable?: boolean;
  nodeKey: string;
  onSave?: (content: string, versionToken: string) => Promise<void>;
  onReload?: () => Promise<void>;
  onRevert?: () => Promise<void>;
};

export function ViewerPane({
  content,
  editable = false,
  versionToken,
  lastSavedBackupAvailable = false,
  nodeKey,
  onSave,
  onReload,
  onRevert,
}: ViewerPaneProps) {
  const [mode, setMode] = useState<"read" | "edit">("read");
  const [draft, setDraft] = useState(content ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    setMode("read");
    setDraft(content ?? "");
    setError("");
  }, [content, nodeKey]);

  const dirty = draft !== (content ?? "");

  async function handleSave() {
    if (!onSave || !versionToken) return;
    setSaving(true);
    try {
      await onSave(draft, versionToken);
      setMode("read");
      setError("");
    } catch (saveError) {
      setError(String(saveError));
    } finally {
      setSaving(false);
    }
  }

  async function handleReload() {
    try {
      await onReload?.();
      setError("");
    } catch (reloadError) {
      setError(String(reloadError));
    }
  }

  async function handleRevert() {
    try {
      await onRevert?.();
      setError("");
      setMode("read");
    } catch (revertError) {
      setError(String(revertError));
    }
  }

  if (mode === "edit" && editable) {
    return (
      <div className="viewer-editor">
        <div className="viewer-toolbar">
          <button onClick={() => setMode("read")}>Cancel</button>
          <button onClick={() => setDraft(content ?? "")} disabled={!dirty || saving}>
            Discard draft
          </button>
          <button onClick={() => void handleReload()} disabled={saving}>
            Reload
          </button>
          <button onClick={() => void handleRevert()} disabled={!lastSavedBackupAvailable || saving}>
            Revert last save
          </button>
          <button onClick={() => void handleSave()} disabled={!dirty || !versionToken || saving}>
            Save
          </button>
        </div>
        {error ? <p className="viewer-error">{error}</p> : null}
        <textarea
          className="viewer-textarea"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          spellCheck={false}
        />
      </div>
    );
  }

  return (
    <div className="viewer-reader">
      {editable ? (
        <div className="viewer-toolbar">
          <button onClick={() => setMode("edit")}>Edit</button>
          <button onClick={() => void handleReload()}>Reload</button>
          <button onClick={() => void handleRevert()} disabled={!lastSavedBackupAvailable}>
            Revert last save
          </button>
        </div>
      ) : null}
      {error ? <p className="viewer-error">{error}</p> : null}
      <pre className="viewer-pre">{content ?? "Select a node."}</pre>
    </div>
  );
}
