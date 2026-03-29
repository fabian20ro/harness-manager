# Iteration Log

> append-only. entry end of every iteration.
> same issue 2+ times? -> promote to `LESSONS_LEARNED.md`.

## Entry Format

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
