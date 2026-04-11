# Lessons Learned

> maintained by AI agents. validated, reusable insights.
> **read start of every task. update end of every iteration.**

## How to Use

- **start of task:** read before writing code - avoid known mistakes
- **end of iteration:** new reusable insight? -> add to appropriate category
- **promotion:** pattern 2+ times in `ITERATION_LOG.md` -> promote here
- **pruning:** obsolete -> Archive section (date + reason). never delete.

---

## Architecture & Design Decisions
**[2026-04-06]** domain-driven file splitting - split large orchestrators (500+ lines) by domain (discovery vs graph vs api) to prevent dependency entanglement. Compose specialized hooks in UI controllers instead of keeping state monolithic.

## Code Patterns & Pitfalls
**[2026-04-06]** JSX duplicate attributes - auto-merges of JSX can silently introduce duplicate attributes (e.g., `role`, `aria-label`) that break TypeScript builds and accessibility-aware tests. Audit JSX changes after merge resolution.

## Testing & Quality
<!-- **[YYYY-MM-DD]** title - explanation -->

## Performance & Infrastructure
**[2026-03-30]** plugin manifest directory expansion - once plugin components (skills, hooks) are modeled explicitly, manifest directory refs should attach to existing component nodes rather than triggering generic recursive file expansion. Prevents graph/memory blow-ups during scans.
**[2026-04-06]** project-agnostic caching - when scanning for global resources (like user-level plugins or global config), ensure the cache key is independent of the current project root. Prevents redundant I/O and CPU-bound discovery during multi-project global scans.

## Dependencies & External Services
<!-- **[YYYY-MM-DD]** title - explanation -->

## Process & Workflow
**[2026-04-11]** standardized `AGENTS.md` preamble - using a consistent "telegraph style" preamble in `AGENTS.md` provides essential non-discoverable constraints (KISS, YAGNI, DRY) that prevent redundant work and ensure consistency across sessions.


---

## Archive
<!-- **[YYYY-MM-DD] Archived [YYYY-MM-DD]** title - reason -->
