# Iteration Log

> append-only. entry end of every iteration.
> same issue 2+ times? -> promote to `LESSONS_LEARNED.md`.

## Entry Format

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
