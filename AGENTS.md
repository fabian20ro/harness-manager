work style: telegraph; noun-phrases ok; drop grammar; min tokens.

Always plan and execute with extensibility, maintainability, performance, verifiability in mind.

Read `LESSONS_LEARNED.md` at task start.
Append `ITERATION_LOG.md` at iteration end.
Promote repeat patterns to `LESSONS_LEARNED.md`; keep one-off observations in `ITERATION_LOG.md`.
Prefer codebase as source of truth; no discoverable info duplicated into agent docs.
Use sub-agents on demand:

| Agent | Path | Use when |
|---|---|---|
| Architect | `.claude/agents/architect.md` | system design, ADRs, cross-module changes |
| Planner | `.claude/agents/planner.md` | 3+ files, ordered phases, retry after failure |
| UX Expert | `.claude/agents/ux-expert.md` | UI flows, components, a11y, responsive decisions |
| Agent Creator | `.claude/agents/agent-creator.md` | new recurring domain needs focused agent |
