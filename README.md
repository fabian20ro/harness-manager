# Harness Inspector

Local-first inspector pentru repo-uri și suprafețe AI coding. Stack:
- helper local în Rust, API pe `http://127.0.0.1:8765`
- UI React/Vite în `ui/`
- store JSON persistent sub `~/.harness-inspector`

## Features v0.1

- project scan din roots configurabile, cu default `~/git`
- suprafețe: Claude Code, Claude Cowork, Codex, Codex CLI, Copilot CLI, IntelliJ/Copilot, OpenCode, Antigravity
- graph intern cu artifacts, refs, plugin-uri, snapshots
- plugin support local pentru Codex și Claude plugin systems
- docs fetch + local snapshot
- observed refresh manual, bazat pe procese locale

## Run

Terminal 1:

```bash
cargo run
```

Terminal 2:

```bash
cd ui
npm install --cache .npm-cache
npm run dev
```

API helper servește și `ui/dist` dacă rulezi:

```bash
cd ui
npm run build
```

apoi deschizi [http://127.0.0.1:8765](http://127.0.0.1:8765).

## Test

```bash
cargo test
```
