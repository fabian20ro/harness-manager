# Iteration Log

> append-only. entry end of every iteration.
> same issue 2+ times? -> promote to `LESSONS_LEARNED.md`.

## Entry Format

---

### [2026-04-11] Periodic AI Agent Config Maintenance

**Context:** audit and synchronize AI agent configuration as per `SETUP_AI_AGENT_CONFIG.md`.
**Happened:**
- **Audit:** Confirmed `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, and sub-agents (`.claude/agents/` and `.gemini/plugins/skills/`) align with latest templates and memory hierarchy.
- **Promotion:** Promoted 3 validated insights (JSX duplicate attributes, plugin manifest expansion, standardized preamble) from `ITERATION_LOG.md` to `LESSONS_LEARNED.md`.
- **Sync:** Updated `ITERATION_LOG.md` to mark promoted entries as `yes`.
- **Validation:** Verified PR template contains the necessary checklist.
**Outcome:** success. Memory layers are lean, non-redundant, and properly categorized.
**Insight:** Regular promotion of insights from `ITERATION_LOG.md` keeps `LESSONS_LEARNED.md` actionable and prevents the log from becoming an information silo.
**Promoted:** no

---

### [2026-04-11] AI Agent Config & Memory System Standardization

**Context:** Project configuration and memory system needed alignment with `SETUP_AI_AGENT_CONFIG.md` guidelines.
**Happened:**
- **Redirects:** Updated `CLAUDE.md` and `GEMINI.md` to `Read AGENTS.md asap`.
- **Standardization:** Updated `AGENTS.md` with the mandatory preamble (development/validation principles) and structured sub-agent table.
- **Audit:** Audited `LESSONS_LEARNED.md` and `ITERATION_LOG.md` for structure and relevance; confirmed compliance.
- **Verification:** Confirmed `.claude/agents/` sub-agents match the latest templates.
**Outcome:** Success. The project's memory hierarchy (AGENTS, LESSONS_LEARNED, ITERATION_LOG, sub-agents) is now standardized and lean.
**Insight:** A clear preamble in `AGENTS.md` provides essential non-discoverable constraints that prevent redundant work and ensure consistency across sessions.
**Promoted:** yes

---

### [2026-04-06] PR #4 Conflict Resolution and PR Verification

**Context:** PR #4 had merge conflicts and auto-merge errors in the UI components and tests.
**Happened:**
- **Conflict Resolution:** Resolved conflicts in `ITERATION_LOG.md`, `CapabilitiesDashboard.tsx`, and `InspectTree.test.tsx`.
- **Bug Fixes:** Fixed a TypeScript error in `InspectTree.tsx` (duplicate `role` and `aria-label` attributes) and updated `GraphInspection.test.tsx` to handle the new accessible labels for directories.
- **Verification:** Verified with `cargo check` and `npm run build && npm test` in the `ui` directory.
- **Outcome:** Success. PR branch is now up-to-date with `main` and verified as green.
**Insight:** Auto-merges of JSX can silently introduce duplicate attributes that break TypeScript builds and accessibility-aware tests.
**Promoted:** yes

---

**Context:** Gemini CLI, Pi Coding Agent, and OpenCode were missing or using outdated configuration patterns. The UI lacked a high-level overview of project "agentic surface area" (skills, hooks, etc.).
**Happened:**
- **Tool Updates:** Integrated Gemini CLI, Pi Coding Agent, and OpenCode into backend catalogs and UI. Researched and applied latest 2026 configuration patterns (e.g., modular `GEMINI.md` with @file, `AGENTS.md` standardization, `.claude/rules/`, `.github/hooks.json`).
- **Backend Refinement:** Updated `AppConfig` with new global directories (~/.gemini, ~/.pi/agent, ~/.opencode, ~/.copilot).
- **UI Dashboard:** Implemented "Agent Capabilities" dashboard to aggregate Skills, Hooks, MCP Servers, and Instructions from the project graph. Updated `MENU_ITEMS` and sidebar navigation.
- **Verification:** Verified with `cargo check` and `npm run build`.
**Outcome:** Success. Project discovery and artifact mapping are now highly accurate for the latest AI tools.
**Insight:** High-level capability aggregation (Skills/Hooks) provides immediate value over raw graph exploration for understanding how an AI agent will interact with a project.
**Promoted:** no

---

### [2026-04-06] Service Layer Modularization (Graph & Tests)

**Context:** `src/services/graph.rs` and `src/services/scan_tests.rs` grew beyond 1300 lines, becoming "god modules" that were hard to navigate and test.
**Happened:**
- **Graph Modularization:** Split `src/services/graph.rs` into a new `services/graph/` directory with specialized modules: `metadata.rs` (skill parsing), `plugins.rs` (discovery), `edges.rs` (graph logic), and `util.rs` (shared helpers).
- **Test Reorganization:** Split `src/services/scan_tests.rs` into `services/scan_tests/` directory grouped by functionality: `discovery.rs`, `indexing.rs`, `references.rs`, and `plugins.rs`.
- **Bug Fixes:** Fixed a type mismatch in node verdict creation and refactored plugin discovery caching to be project-agnostic for global plugins, improving cache hit rates.
- **Verification:** Verified all 21 integration tests pass in the new modular structure.
**Outcome:** Success. Codebase complexity is significantly reduced; service layer is now clean and extensible.
**Insight:** When modularizing, ensure cache keys for global resources are project-agnostic to prevent redundant I/O across different project scans.
**Promoted:** yes
### [2026-04-06] UI Recreation and Modernization

**Context:** The UI was functional but lacked visual hierarchy and professional polish. User goal: "find projects, analyze per editor/ai combo, view those files, edit/revert ai harnesses".
**Happened:** 
- **Backend Fix:** Identified and fixed a JSON deserialization bug in the graph endpoint (missing `byte_size` field in `ArtifactNode`). Added `#[serde(default)]` to `src/domain.rs`.
- **Design System:** Created a new slate-based design system with modern typography (Inter), vibrant accents (#3b82f6), and consistent spacing/roundness.
- **Layout Refactor:** Rebuilt the `App` shell and `Inspect` grid. Switched from a generic 3-panel layout to a professional IDE-style 4-column split (Sidebar + 3-panel Inspect).
- **Component Modernization:** 
  - `SidebarNav`: Sleek icons and better active states.
  - `InspectToolbar`: Compact, grouped controls with improved field styling.
  - `InspectTree`: Modernized tree with better indentation, hover states, and state-aware icons.
  - `ViewerPane` & `InspectReasonsPane`: Integrated into the new panel system with consistent header/body structure.
- **Validation:** Used Playwright MCP to capture screenshots/snapshots, verify accessibility, and confirm the new look meets the "Ease of task" goal.
**Outcome:** Success. The UI feels much more like a high-end development tool, providing clear focus on project discovery and artifact inspection.
**Insight:** A clean, grid-based layout for multi-panel inspection reduces cognitive load. Even without a formal design tool like Stitch (due to auth issues), applying consistent design tokens and layout rules (Architect/UX Expert) significantly improves the perceived quality and usability of the application.
**Promoted:** no

---

### [2026-04-06] Monolithic file split and modularization

**Context:** codebase reached 500+ line files (`scan.rs` > 3800 lines); violated Architect boundaries; hard to maintain/test
**Happened:** split `src/services/scan.rs` into `plugins/discovery.rs`, `projects/discovery.rs`, `graph.rs`, and `scan_tests.rs`; refactored `src/services/refs.rs` into `services/refs/` directory with specialized format modules; split `src/api.rs` into `api/` directory by resource; modularized `ui/src/hooks/useInspectController.ts` into specialized hooks under `hooks/inspect/`; split `ui/src/App.test.tsx` into focused feature tests; updated all imports and verified with `cargo check` and `npm run build`
**Outcome:** success; `scan.rs` reduced by ~90%; clear domain boundaries; faster test isolation
**Insight:** split large orchestrators early by domain (discovery vs graph vs api) to prevent dependency entanglement; compose specialized hooks in UI controllers instead of keeping state monolithic
**Promoted:** yes

---

### [2026-03-30] Plugin manifest directory expansion caused scan blow-up

**Context:** after memoizing plugin candidate discovery, global reindex still appeared frozen on `Discovering Codex plugins for ~/git/ComfyUI-Chibi-Nodes`; a live process sample showed the real hot path was `collect_reference_edges -> materialize_referenced_directory`, with physical footprint around 1.1 GB and peak around 10.9 GB during scan
**Happened:** reproduced the scan locally against the real home/plugin roots; confirmed the visible Codex line was stale and the worker was CPU-bound inside plugin-manifest directory expansion; changed plugin-manifest directory refs to link only already-modeled descendant artifacts instead of recursively materializing every file under referenced directories like `skills/`; kept recursive directory expansion for non-plugin-manifest refs; updated the Codex directory-reference regression to assert existing skill artifacts stay linked while unrelated files inside the directory are not materialized; verified `cargo test`; reran a real local `/api/scan` and confirmed progress advanced past `ComfyUI-Chibi-Nodes` onto later projects instead of pinning there
**Outcome:** success
**Insight:** once plugin components are modeled explicitly, manifest directory refs should attach to those existing component nodes, not trigger generic recursive file expansion; otherwise plugin bundles create graph/memory blow-ups that look like frozen scans
**Promoted:** yes

... rest of file ...

## Iteration 1 - 2026-04-08
- Removed unused 'export' from 'ToolContext' in 'ui/src/lib/types.ts'.
- Added 'useMemo' to 'CapabilitiesDashboard.tsx' to optimize artifact filtering.
- Observed that 'ToolContext' is used by 'SurfaceState' in the same file, so it was kept as a private type.
- Network restrictions in the sandbox prevented running full build and tests, but static analysis confirmed safety.
