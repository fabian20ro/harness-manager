# Harness Inspector

Harness Inspector = local-first inspector for AI coding harnesses.

Current shape:
- Rust helper on `127.0.0.1:8765`
- React/Vite UI in [`/Users/fabian/git/harness-manager/ui`](./ui)
- JSON store under `~/.harness-inspector`
- graph-backed inspection model; UI = tree/graph projection over same data

## How it works now

Runtime split:
- helper scans repo roots, global dirs, plugin install roots, docs snapshots
- helper builds per-tool `SurfaceState` with nodes, edges, verdicts
- UI reads helper API locally
- GitHub Pages hosts only static UI; Pages UI still talks to local helper by default

Main tabs:
- `Projects`: discovered git repos from configured roots; default root = `~/git`
- `Docs`: fetch remote docs, save normalized local snapshots, attach to selected project/tool
- `Tool`: choose surface context
- `Inspect`: effective context tree, viewer, reasons, refs
- `Activity`: manual `observed` refresh from local process evidence

Supported tool contexts:
- Claude Code
- Claude Cowork
- Codex
- Codex CLI
- Copilot CLI
- IntelliJ/Copilot
- OpenCode
- Antigravity

Plugin support now:
- local Codex plugins
- local Claude/Cowork plugin system
- plugin manifests + plugin docs become graph nodes
- compatibility edges shown where catalog says so

## Truth model

The helper does not collapse everything into “active”.

Primary states:
- `declared`
- `effective`
- `observed`

Additional states:
- `referenced_only`
- `shadowed`
- `ignored`
- `misleading`
- `inactive`
- `unresolved`
- `broken_reference`
- `installed`
- `configured`

Current effective behavior:
- seed artifacts come from tool catalog rules + known locations
- base-file refs and typed config refs can recursively promote downstream files into `effective`
- generic text mentions stay exploratory by default and do not promote into effective closure

Current reference intelligence:
- base instruction files such as `AGENTS.md` / `CLAUDE.md`
- typed TOML / JSON / YAML config fields
- plugin manifests
- generic markdown / quoted / import-like fallback refs

## Helper API + storage

Current API:
- `GET /api/projects`
- `POST /api/scan`
- `GET /api/projects/:id/graph?tool=...`
- `GET /api/projects/:id/inspect?tool=...&node=...`
- `POST /api/docs/fetch`
- `POST /api/activity/refresh`
- `POST /api/catalogs/refresh`
- `GET /api/jobs/:id`
- `GET /api/events`

Persistent store layout:
- `settings.json`
- `roots.json`
- `catalogs/<surface>/<version>.json`
- `projects/<project-id>/inventory.json`
- `projects/<project-id>/graph.nodes.json`
- `projects/<project-id>/graph.edges.json`
- `projects/<project-id>/tool-state/<surface>.json`
- `snapshots/...`
- `activity/...`

Catalogs are versioned JSON. They define known locations, artifact rules, plugin systems, and observed probes. Current refresh path is manual and schema-driven.

## Local dev

Helper:

```bash
cargo run
```

UI dev:

```bash
cd ui
npm install --cache .npm-cache
npm run dev
```

Built UI through helper:

```bash
cd ui
npm run build -- --emptyOutDir
```

Then open [http://127.0.0.1:8765](http://127.0.0.1:8765).

## GitHub Pages + Releases

Pages:
- Pages publishes static UI from `ui/dist`
- Pages build emits `build-meta.json`
- browser checks every 180s, and when tab becomes visible again, for newer deploys
- if newer build exists, page reloads with cache-busting query param

Pages/local-helper contract:
- default API base on `github.io` = `http://127.0.0.1:8765`
- API base can also come from `?apiBase=...`
- chosen API base persists in `localStorage`

Releases:
- GitHub Releases publish macOS helper archives for `arm64` and `x64`
- for local development, normal path stays `cargo run`

## Security model

Current defaults:
- helper binds only to `127.0.0.1:8765`
- CORS allowlist only; includes localhost dev and Pages origin
- docs fetch is HTTPS-only by default
- docs fetch blocks localhost, loopback, and private-network targets
- snapshot fetch has timeout, redirect cap, and byte cap

Relevant env overrides:
- `HARNESS_ALLOWED_ORIGINS`
- `HARNESS_ALLOW_INSECURE_DOC_HOSTS=true`

## Limitations

Current limitations:
- macOS only
- read-only product; no write-back
- `observed` = best-effort manual refresh, not full runtime truth
- OpenCode / Antigravity coverage still seed-catalog quality
- semantic reference extraction is targeted, not general language intelligence

## Test

Backend:

```bash
cargo test
```

UI:

```bash
cd ui
npm test
```
