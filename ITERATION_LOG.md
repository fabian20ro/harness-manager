# Iteration Log

> append-only. entry end of every iteration.
> same issue 2+ times? -> promote to `LESSONS_LEARNED.md`.

## Entry Format

---

### [2026-03-29] Catalog-driven project discovery signals

**Context:** project discovery had already expanded beyond `.git`, but the non-git rules still lived in Rust heuristics, so adding Codex/Copilot/Antigravity package/workspace signals from docs would keep forcing scanner code edits and cross-surface bugs
**Happened:** added `project_discovery_rules` to the catalog schema with discovery kind, score, reason, root strategy, and scan-root skip behavior; refactored project candidate discovery to compile catalog rules once and apply them generically while keeping `.git` as the only built-in root signal; moved Codex, Copilot CLI, IntelliJ/Copilot, Claude, and conservative Antigravity discovery signals into seed catalogs; constrained Codex/Claude package rules to their global plugin roots to avoid cross-surface `SKILL.md` leakage; added regressions for Copilot skill packages, hooks non-promotion, duplicate-signal merge, and kept existing git/package precedence; updated README and verified `cargo test`
**Outcome:** success
**Insight:** discovery signals need surface scoping in data, not just path-shape matching; generic `SKILL.md` rules become wrong immediately once multiple harness ecosystems share the same filename conventions
**Promoted:** no

---

### [2026-03-29] Hybrid project discovery with plugin package tier

**Context:** project discovery only accepted `.git` roots, so real non-git harness workspaces and installed plugin packages never appeared in Projects; broadening discovery risked flooding the list with weak-signal directories
**Happened:** replaced repo-only root detection with scored candidate discovery across configured roots and known global dirs; added three project kinds (`git_repo`, `workspace_candidate`, `plugin_package`) plus discovery reason/score on `ProjectSummary`; promoted non-git workspaces only from strong signals (`AGENTS.md`, `CLAUDE.md`, `.codex/config.toml`, `.claude/config.json`, Copilot repo files); promoted plugin packages from plugin manifests, `.mcp.json`, or `SKILL.md`; kept weak signals from creating workspace candidates on their own; made git roots outrank nested plugin/workspace candidates; updated Projects UI and toolbar labels to show tiers; added backend/UI regressions; verified `cargo test`, `npm test -- --run`, `npm run build`
**Outcome:** success
**Insight:** candidate-root discovery needs stricter evidence than artifact discovery; if every interesting file can mint a project, the Projects list loses trust immediately, so discovery must model signal strength and project kind explicitly
**Promoted:** no

---

### [2026-03-29] Codex plugin skills as first-class artifacts

**Context:** Codex plugin `skills` paths resolved on disk, but the graph still treated them as generic reference targets; imported plugin docs also assumed stale frontmatter-based Codex discovery instead of the current `SKILL.md` + `agents/openai.yaml` contract
**Happened:** extended Codex plugin scanning to emit explicit `skill` plugin artifacts from manifest-declared `skills` paths; supported direct `SKILL.md` paths and recursive skill-directory discovery; parsed `SKILL.md` frontmatter `name` / `description`; attached optional `agents/openai.yaml` metadata plus legacy frontmatter keys as compatibility-only metadata; kept manifest-relative generic refs for secondary files; updated inspect tree leaf labeling to prefer skill names; switched default Codex docs URL to the skills docs; updated README contract notes; added backend and UI regressions; verified `cargo test`, `npm test -- --run`, `npm run build`
**Outcome:** success
**Insight:** plugin component existence and plugin reference traversal are different layers; once components like skills are modeled directly instead of inferred from generic reference edges, metadata, labeling, and broken-path handling all become straightforward
**Promoted:** no

---

### [2026-03-29] Tree collapse fix + plugin-root-relative manifest refs

**Context:** inspect tree collapse sometimes failed; first-load tree not fully expanded; Codex/Claude plugin manifest refs pointed into `.codex-plugin` / `.claude-plugin` marker dirs instead of plugin roots, breaking skills/agents/commands reasons
**Happened:** removed steady-state forced-expanded ancestor behavior from inspect tree state and switched to one-shot ancestor auto-expand plus full first-load expansion seed; added directory-key helpers/tests; extended plugin artifact metadata with root-relative resolver context; updated scanner reference traversal to preserve plugin-root resolution; normalized plugin discovery around catalog `manifest_paths` instead of marker-dir-only assumptions; kept Codex/Claude extra discovery roots but reused the same manifest-path derivation; added regression coverage for Codex/Claude manifest refs and verified `cargo test`, `npm test -- --run`, `npm run build`
**Outcome:** success
**Insight:** selection visibility and branch expansion must be separate concerns; plugin manifests need provenance path and resolution base path tracked independently, or inspect reasons drift from the real filesystem layout
**Promoted:** no

---

### [2026-03-29] Inspect UX refactor + scoped project/tool reindex

### [2026-03-29] Tree collapse fix + plugin-root manifest resolution

**Context:** inspect tree collapse still blocked on selected ancestors; plugin skills/agents/commands resolved via injected `.codex-plugin` / `.claude-plugin` segments, breaking real local plugin paths
**Happened:** removed persistent forced-expansion model; defaulted first-load tree state to all directories expanded; kept one-shot ancestor auto-expand on selection without blocking later collapse; added app/tree regressions for collapse and stored expansion behavior; extended resolver context with explicit `resolve_from_dir`; carried plugin artifact resolution base path from scanner; switched plugin discovery to manifest-path-driven root derivation; updated Claude seed catalogs to include `.claude-plugin/plugin.json`; added Codex/Claude resolver + surface-state regressions; verified `cargo test`, `npm test -- --run`, `npm run build`
**Outcome:** success
**Insight:** tree UX and plugin reference correctness both need explicit base state instead of inferred transient state; once expansion seed and manifest-relative base dir are modeled directly, hidden side effects disappear
**Promoted:** no

---

**Context:** inspect UX had layout/affordance problems: weak sidebar brand, misplaced collapse control, helper mismatch, double-scroll feel, non-collapsible tree, weak reindex visibility, no scoped reindex, wrong inspect column ratios
**Happened:** added scoped `POST /api/projects/:id/reindex` backed by project inventory refresh + single-surface rebuild + union-graph rewrite; extended `JobStatus` with scope metadata; extracted UI inspect controller hook for persisted selection/fetch/SSE/reindex orchestration; rebuilt sidebar into brand/nav/footer; moved global reindex to footer and collapse to bottom; restyled top toolbar/helper; replaced compressed inspect trie with explicit expandable directory tree + persisted expansion state; converted inspect layout to 4-column desktop split with viewport-bound workspace and pane scrolling; strengthened scan status copy; expanded backend/frontend regression tests; verified `cargo test`, `npm test -- --run`, `npm run build`
**Outcome:** success
**Insight:** tree collapse and scoped refresh both depend on stable per-scope identifiers; once job payloads and UI storage keys carry explicit `project + tool` scope, reindex state, refresh invalidation, and tree persistence stay simple
**Promoted:** no

---

### [2026-03-29] Reindex bottom progress bar + live SSE scan status

**Context:** reindex UX weak; only static sidebar message; no live folder-level feedback during scan
**Happened:** extended `JobStatus` with scan progress fields; added `JobRegistry.update`; threaded progress callbacks through repo discovery/inventory/surface scan; added throttled SSE job updates; moved scan status out of sidebar into fixed bottom bar; subscribed UI to `/api/events`; added backend/frontend regression tests
**Outcome:** success
**Insight:** long local scans need event-driven progress from the scanner itself, not optimistic frontend copy; path-change-triggered emits give useful freshness without spamming storage/SSE
**Promoted:** no

---

### [2026-03-29] Inspect write mode + stronger refs + real plugin discovery

**Context:** close inspect gaps: `CLAUDE.md` docs-map refs, tree usage signal, actual local plugin installs, local edit/save/revert flow
**Happened:** upgraded instruction-doc resolver for directives/tables/code spans; added recursive effective promotion coverage; replaced catalog-only plugin lookup with Codex cache/tmp and Claude installed-index/marketplace discovery; added inspect edit API and UI with save/conflict/revert; added tree used/unused/broken indicators; added backend/frontend regression tests
**Outcome:** success
**Insight:** docs-as-instructions need their own strong-ref parser; plugin docs often lag real install layouts, so local indexes/layout heuristics must outrank seed catalogs
**Promoted:** no

---

### [2026-03-28] AI agent config setup

**Context:** bootstrap repo memory hierarchy and sub-agent files from setup guide
**Happened:** added minimal `AGENTS.md`, redirect `CLAUDE.md`, sub-agent specs, `LESSONS_LEARNED.md`, `ITERATION_LOG.md`, setup guide copy, PR template
**Outcome:** success
**Insight:** keep `AGENTS.md` small; repo-discoverable facts stay out
**Promoted:** no

---

<!-- new entries above this line, most recent first -->
### [2026-03-29] Inspect tree directory expansion + inline status + inspect 404s

**Context:** inspect tree stopped at directory refs like plugin `skills`; bottom reindex bar duplicated status and wasted space; stale `Inspect failed: 500` copy survived after later successful loads
**Happened:** expanded scan-time directory reference targets into descendant file nodes while preserving the directory node for reasons/provenance; added transitive directory-to-file reference edges so effective status flows into nested files; changed inspect API missing-node handling from generic internal error to `404`; moved scan status to a single inline notice and removed fixed bottom status usage; added tree `Expand all` / `Collapse all`; cleared inspect error state on new/successful fetches and surfaced API error text with node-aware copy; added backend/frontend regressions; verified `cargo test`, `npm test -- --run`, `npm run build`
**Outcome:** success
**Insight:** directory references need to be modeled in the graph, not inferred in the tree alone; once descendant files are explicit nodes, viewer/reasons/error handling stay consistent with selection and status UX
**Promoted:** no

---
