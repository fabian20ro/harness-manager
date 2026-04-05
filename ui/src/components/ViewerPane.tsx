import { useEffect, useState } from "react";

type ViewerPaneProps = {
  content?: string;
  metadata?: Record<string, unknown>;
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
  metadata,
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

  const [showGitignore, setShowGitignore] = useState(false);

  useEffect(() => {
    setMode("read");
    setDraft(content ?? "");
    setError("");
    setShowGitignore(false);
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

  const contexts = Array.isArray(metadata?.contexts) ? metadata.contexts as Array<{type: string, content: string}> : [];
  const globalMemories = typeof metadata?.global_memories === "string" ? metadata.global_memories : null;
  const adjacentGitignore = typeof metadata?.adjacent_gitignore === "string" ? metadata.adjacent_gitignore : null;

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
      <div className="viewer-toolbar">
        {editable ? (
          <>
            <button onClick={() => setMode("edit")}>Edit</button>
            <button onClick={() => void handleReload()}>Reload</button>
            <button onClick={() => void handleRevert()} disabled={!lastSavedBackupAvailable}>
              Revert last save
            </button>
          </>
        ) : null}
        {adjacentGitignore ? (
          <button 
            style={{ marginLeft: editable ? "auto" : 0 }} 
            onClick={() => setShowGitignore(!showGitignore)}>
            {showGitignore ? "Show .geminiignore" : "Compare with .gitignore"}
          </button>
        ) : null}
      </div>
      {error ? <p className="viewer-error">{error}</p> : null}

      {contexts.length > 0 && !showGitignore && (
        <div className="viewer-metadata-section">
          <h3>Context Precedence</h3>
          {contexts.map((ctx, i) => (
            <div key={i} className="context-block">
              <span className="context-tag">&lt;{ctx.type}&gt;</span>
              <pre className="context-content">{ctx.content}</pre>
            </div>
          ))}
        </div>
      )}

      {globalMemories && !showGitignore && (
        <div className="viewer-metadata-section viewer-memories">
          <h3>Global Memories</h3>
          <pre className="context-content">{globalMemories}</pre>
        </div>
      )}

      {showGitignore ? (
        <div className="diff-view" style={{ display: 'flex', gap: 10, height: '100%' }}>
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column' }}>
            <h4 style={{ margin: "0 0 5px" }}>.geminiignore</h4>
            <pre className="viewer-pre">{content ?? "Select a node."}</pre>
          </div>
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column' }}>
            <h4 style={{ margin: "0 0 5px" }}>.gitignore</h4>
            <pre className="viewer-pre" style={{ background: "rgba(31, 111, 178, 0.1)" }}>{adjacentGitignore}</pre>
          </div>
        </div>
      ) : (
        <pre className="viewer-pre">{content ?? "Select a node."}</pre>
      )}
    </div>
  );
}
