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

## [2026-04-13] Fix TypeScript 'bool' Error
- fixed: TS2304: Cannot find name 'bool' in ui/src/lib/types.ts.
- action: replaced 'bool' with 'boolean' in 'CheckResult' type.
- verification: 'npm run build' in 'ui/' directory passed.

## [2026-05-11] README tab list sync
- changed: added the missing `Capabilities` tab to the README main-tabs list.
- reason: README now matches the current UI shell and avoids stale discoverability docs.
- verification: direct file review only; no code path changed.

## [2026-05-12] Explicit button types for shared UI controls
|- changed: added `type="button"` to shared nav / toolbar / helper buttons and tightened component tests to assert it.
|- reason: plain buttons default to submit behavior inside forms; making intent explicit avoids accidental form submission if these controls are reused in a form shell later.
|- verification: `npm test -- --run src/components/HelperCommand.test.tsx src/components/SidebarNav.test.tsx src/components/InspectToolbar.test.tsx`; `npm run build`.
|- outcome: success. UI behavior preserved; reusable controls are safer by default.
|- insight: shared buttons should declare their type even when the current layout is not form-based.
|- promoted: no
|-

## [2026-05-12] Scan conflict error now tells users to wait and retry
|- changed: clarified the 409 conflict message for scan/reindex collisions in `src/api/projects.rs` and kept the UI test fixture in sync.
|- reason: the old message was accurate but not actionable; the new wording tells users the next step without changing behavior.
|- verification: `PATH=/opt/rust/cargo/bin:$PATH CARGO_HOME=/tmp/harness-manager-cargo-home cargo test scan_start_rejects_when_another_scan_job_is_running -- --nocapture`; `npm test -- --run src/tests/ProjectScanning.test.tsx`.
|- outcome: success. Backend contract and UI expectation now match the clearer retry guidance.
|- insight: short retry guidance belongs in user-facing conflict text when the action is safe to repeat.
|- promoted: no
|-

## [2026-05-13] Capabilities empty state now tells users what to select
|- changed: updated the Capabilities dashboard empty state copy in `ui/src/components/CapabilitiesDashboard.tsx` and added `ui/src/components/CapabilitiesDashboard.test.tsx`.
|- reason: the previous empty state was correct but vague; the new copy names the selection needed and previews the kinds of capabilities the panel shows.
|- verification: `npm test -- src/components/CapabilitiesDashboard.test.tsx`; `npm run build` in `ui/`.
|- outcome: success. Empty state is now more actionable, and the component is covered by a focused regression test.
|- insight: empty states work better when they say both what to do next and what will appear after selection.
|- promoted: no
---

## [2026-05-13] Capabilities dashboard now handles an empty discovered set
- changed: added an explicit empty state in `ui/src/components/CapabilitiesDashboard.tsx` for projects/tool contexts with no discovered capability nodes, and added a regression test in `ui/src/components/CapabilitiesDashboard.test.tsx`.
- reason: without this guard the dashboard rendered an empty grid shell, which looked broken instead of intentionally empty.
- verification: `npm exec vitest -- run src/components/CapabilitiesDashboard.test.tsx`; `npm run build` in `ui/`.
- outcome: success. The panel now tells users when there is nothing to show yet, and the UI build stays green.
- insight: blank capability dashboards need a dedicated no-data state, not just per-section null returns.
- promoted: no


## [2026-05-13] Capabilities dashboard accessibility polish
|- Added `role="status"`, `aria-live="polite"`, and `aria-atomic="true"` to the two Capabilities dashboard empty states so the panel can announce selection/no-results changes.
|- Added focused tests that assert the live-region contract on both empty states.
|- Verified with `npm test -- --run src/components/CapabilitiesDashboard.test.tsx` in `ui/`.

## [2026-05-13] Learning-loop reference names synced to current repo conventions
|- changed: updated the reference-resolution regression fixtures in `src/services/refs/mod.rs` and `src/services/scan_tests/references.rs` to use `LESSONS_LEARNED.md` and `ITERATION_LOG.md` instead of legacy `ANALYSIS.md` / `TODOS.md` names.
|- reason: the repo's active learning loop uses `LESSONS_LEARNED.md` and `ITERATION_LOG.md`; the tests should exercise the current instruction surface rather than stale filenames.
|- verification: `PATH=/opt/rust/cargo/bin:$PATH CARGO_HOME=/tmp/harness-manager-cargo-home cargo test sentence_style_instruction_references_become_effective`; `PATH=/opt/rust/cargo/bin:$PATH CARGO_HOME=/tmp/harness-manager-cargo-home cargo test extracts_multiple_instruction_directives_from_sentence`.
|- outcome: success. The reference tests now mirror the repo's current learning-loop convention.
|- insight: when a repo's durable workflow names change, fixture text should track the live convention so parser tests stay representative.
|- promoted: no
|
## [2026-05-14] README supported tool contexts synced to current tool list
||- changed: added the missing Gemini CLI and Pi Coding Agent entries to the README supported tool contexts list.
||- reason: `ui/src/lib/inspect.ts` already exposes those tool IDs and labels, so the README was under-reporting supported contexts.
||- verification: direct file review against `ui/src/lib/inspect.ts`; docs-only change, no runtime tests run.
||- outcome: success. README now matches the current tool coverage surfaced by the UI.
||- insight: when a public docs list is derived from a central tool enum, sync both sides together instead of leaving the README as a stale subset.
||- promoted: no
||
## [2026-05-14] Viewer pane buttons now declare explicit button type
|- changed: added `type="button"` to the ViewerPane edit, reload, revert, toggle, and health-fix controls; added a focused regression test for the read/edit controls.
|- reason: these controls are click-only UI actions and explicit button types prevent accidental form-submit behavior if the component is embedded in a form later.
|- verification: `npm test -- --run src/components/ViewerPane.test.tsx`; `npm run build` in `ui/`.
|- outcome: success. The viewer controls keep the same UX while becoming safer by default.
|- insight: even leaf UI controls benefit from explicit button types when the component may be reused in a form shell.
|- promoted: no

## [2026-05-14] App shell buttons now declare explicit button type
|- changed: added `type="button"` to the Projects, Docs, Inspect, and Activity shell buttons in `ui/src/App.tsx`; added a regression test in `ui/src/tests/AppNavigation.test.tsx` that checks the button type across those visible tabs.
|- reason: shell-level click actions should be explicit about not submitting forms, matching the repo's prior button-type hardening pattern.
|- verification: `npm test -- --run src/tests/AppNavigation.test.tsx`; `npm run build` in `ui/`.
|- outcome: success. The app shell actions are safer by default, and the regression test pins the contract.
|- insight: even when a component is outside a form today, explicit `type="button"` keeps click-only controls safe if the component is later embedded inside a form shell.
|- promoted: yes

## [2026-05-14] Context-cost boundary coverage for scan status
|- changed: added a focused Vitest regression in `ui/src/lib/inspect.test.ts` covering `calculateContextCost()` at the exact 200 KB boundary and one byte above it.
|- reason: `ScanStatusBar` warns on `> 200 * 1024`; the boundary behavior was not pinned by tests.
|- verification: `npm exec vitest -- run ui/src/lib/inspect.test.ts`; `npm run build` in `ui/`.
|- outcome: success. The context-size heuristic is now regression-tested at the cutoff users actually see.
|- insight: boundary-only warnings should have an exact-limit test, not just a generic large-input case.
|- promoted: no

## [2026-05-14] URL-like paths now survive display normalization
||- changed: updated `formatDisplayPath()` in `ui/src/lib/inspect.ts` to preserve URL-like strings while still normalizing redundant slashes in filesystem paths; added a regression test in `ui/src/lib/inspect.test.ts`.
||- reason: path display should not mangle `https://`-style strings if they surface through graph data or future UI reuse.
||- verification: `npm exec vitest -- run src/lib/inspect.test.ts`; `npm run build` in `ui/`.
||- outcome: success. Normal filesystem paths still normalize, and URL-like paths now render unchanged.
||- insight: generic path-format helpers should treat URI schemes as a separate contract from local-path cleanup.
||- promoted: no
|
## [2026-05-15] README current API list synced to backend routes
- changed: added the missing `POST /api/projects/:id/reindex` and inspect persistence/fix routes to the README current API list.
- reason: the backend router already exposes these endpoints, so the docs were under-reporting the live helper surface.
- verification: direct route-table review against `src/api/mod.rs`; docs-only change, no runtime tests run.
- outcome: success. README now reflects the current helper API endpoints more completely.
- insight: when a public API list is derived from the router, keep the docs synced with newly exposed routes to avoid stale discoverability.
- promoted: no

---

## [2026-05-15] README app shell route made discoverable
- changed: documented `GET /` in the README API list as the app shell entry point.
- reason: the router already serves the UI shell at `/`, but the README only listed helper endpoints.
- verification: direct route-table review against `src/api/mod.rs`; docs-only change, no runtime tests run.
- outcome: success. The visible landing route is now discoverable alongside the helper API.
- insight: when the local helper also serves the UI shell, the root route belongs in the API surface docs.
- promoted: no

## [2026-05-15] README scan-status warning made discoverable
- changed: documented the Gemini truncation warning in the README current-shape summary.
- reason: the UI already surfaces effective context size and warns near the ~200 KB limit, but the README did not mention that user-visible contract.
- verification: direct file review against `ui/src/components/ScanStatusBar.tsx` and `ui/src/lib/inspect.ts`; docs-only change, no runtime tests run.
- outcome: success. The README now mentions the scan-status warning users can see in the helper UI.
- insight: when a status bar encodes a threshold warning, the same threshold belongs in the top-level product summary.
- promoted: no

## [2026-05-16] README scan-status threshold precision synced to code
- changed: tightened the README wording to say the warning triggers above 200 KB, matching `calculateContextCost()`.
- reason: the UI copy says "approaching" the limit, but the actual heuristic is `bytes > 200 * 1024`; the docs should match the exact contract.
- verification: direct file review against `ui/src/lib/inspect.ts` and `ui/src/components/ScanStatusBar.tsx`; docs-only change, no runtime tests run.
- outcome: success. The README now matches the implementation threshold instead of implying a softer boundary.
- insight: threshold-style warnings should document the exact trigger condition, not just the user-facing tone.
- promoted: no

## [2026-05-16] README status-bar readout label synced to code
- changed: updated the README scan-status bullet to mention the literal `Effective context size` readout in `ui/src/components/ScanStatusBar.tsx`.
- reason: the status bar already shows the exact metric label, but the docs only described the warning tone and threshold.
- verification: direct file review against `ui/src/components/ScanStatusBar.tsx`; docs-only change, no runtime tests run.
- outcome: success. The README now covers both the readout label and the warning threshold users actually see.
- insight: when a visible status bar combines a label with a threshold warning, document both together so the discoverability note matches the shipped UI.
- promoted: no

## [2026-05-17] Index fallback tests isolated from repo UI build output
- changed: moved `src/api/meta.rs` index tests into a temporary cwd helper so they no longer create or delete `ui/dist/index.html` in the working tree.
- reason: tests should not mutate repo-local build output paths; isolated temp dirs make fallback/file-serving cases deterministic and avoid cleanup coupling.
- verification: `PATH=/opt/rust/cargo/bin:$PATH CARGO_HOME=$(mktemp -d -t harness-cargo-XXXXXX) cargo test test_index --quiet`.
- outcome: success. Both index tests pass against isolated filesystem state.
- insight: tests for relative-path fallback behavior should change cwd into a temp dir rather than editing real repo-relative paths.
- promoted: no

## [2026-06-06] Stale tracked patch backups removed
- changed: removed three tracked `.orig` snapshots beside the active InspectTree and GraphInspection sources.
- reason: the snapshots differed from current sources, had no repository references, and were patch artifacts rather than maintained fixtures.
- verification: repository reference search; `npm test -- --run` (12 files, 41 tests); `npm run build`.
- outcome: success. Active sources remain intact; tests and production build pass without the backups.
- insight: patch backup files should remain untracked; active source and version history already preserve recoverable states.
- promoted: no
