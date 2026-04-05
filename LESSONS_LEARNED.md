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
<!-- **[YYYY-MM-DD]** title - explanation -->

## Testing & Quality
<!-- **[YYYY-MM-DD]** title - explanation -->

## Performance & Infrastructure
<!-- **[YYYY-MM-DD]** title - explanation -->

## Dependencies & External Services
<!-- **[YYYY-MM-DD]** title - explanation -->

## Process & Workflow
<!-- **[YYYY-MM-DD]** title - explanation -->

---

## Archive
<!-- **[YYYY-MM-DD] Archived [YYYY-MM-DD]** title - reason -->
