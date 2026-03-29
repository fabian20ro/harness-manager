# Iteration Log

> append-only. entry end of every iteration.
> same issue 2+ times? -> promote to `LESSONS_LEARNED.md`.

## Entry Format

---

### [2026-03-29] Inspect UX refactor + scoped project/tool reindex

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
