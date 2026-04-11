# AGENTS.md

work style: telegraph; noun-phrases ok; drop grammar; min tokens.

> bootstrap context only
- discoverable from codebase → don't put here.
> corrections + patterns → LESSONS_LEARNED.md.
> development:
- correctness first
- smallest good change
- preserve behavior / interfaces / invariants unless task says otherwise
- simple, explicit code
- KISS
- YAGNI
- DRY; rule of three; temp duplication ok during migration
- high cohesion; low coupling
- follow repo patterns unless intentionally replacing with better consistent one
- refactor when patch would raise future complexity
- for broad changes: optimize for coherent end-state; stage changes; each step verifiable
- no unrelated churn
- leave code better
> validation:
- fastest relevant proof
- targeted tests first
- typecheck / build / lint as needed
- smoke tests for affected flows when useful
- update tests when behavior intentionally changes
> ambiguity:
- cannot decide from code -> explain; ask; no assume
- otherwise choose most reversible reasonable path; state assumption

---

Read `LESSONS_LEARNED.md` at task start.
Append `ITERATION_LOG.md` at iteration end.
Promote repeat patterns to `LESSONS_LEARNED.md`; keep one-off observations in `ITERATION_LOG.md`.

## Sub-Agents

| Agent | Path | Use when |
|---|---|---|
| Architect | `.claude/agents/architect.md` | system design, ADRs, cross-module changes |
| Planner | `.claude/agents/planner.md` | 3+ files, ordered phases, retry after failure |
| UX Expert | `.claude/agents/ux-expert.md` | UI flows, components, a11y, responsive decisions |
| Agent Creator | `.claude/agents/agent-creator.md` | new recurring domain needs focused agent |
