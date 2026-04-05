# Iteration Log

> append-only. entry end of every iteration.
> same issue 2+ times? -> promote to `LESSONS_LEARNED.md`.

## Entry Format

---

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
**Promoted:** no

... rest of file ...
