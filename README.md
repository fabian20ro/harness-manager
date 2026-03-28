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

## GitHub Pages + Releases

- GitHub Pages publică UI-ul static din `ui/dist`.
- UI-ul publicat pe Pages presupune că helper-ul local rulează pe `http://127.0.0.1:8765`.
- API base poate fi schimbat din UI, din query string `?apiBase=...`, sau din `localStorage`.
- build-ul publicat include `build-meta.json`; pagina verifică la 180s și la revenirea în tab dacă există deploy nou și face reload cu cache-busting query param.
- Workflow-ul `Deploy Pages` publică UI-ul pe Pages la push pe `main`.
- Workflow-ul `Release Helper` publică arhive macOS (`arm64` și `x64`) în GitHub Releases la tag-uri `v*` sau manual.

## Security defaults

- helper-ul ascultă doar pe `127.0.0.1:8765`
- CORS allowlist: localhost dev + `https://fabian20ro.github.io`
- docs fetch permite implicit doar `https://` și blochează `localhost`, loopback și adrese private
- override-uri:
  - `HARNESS_ALLOWED_ORIGINS`
  - `HARNESS_ALLOW_INSECURE_DOC_HOSTS=true`

## Test

```bash
cargo test
```
