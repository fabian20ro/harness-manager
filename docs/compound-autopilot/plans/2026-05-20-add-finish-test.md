# Plan: Add regression test for JobRegistry::finish timestamping

## Goal
Ensure that calling `JobRegistry::finish` correctly updates the `finished_at` field of the `JobStatus`.

## Current Behavior
The `finish` method exists, but there is no automated verification that it sets the `finished_at` timestamp.

## Implementation Units

### Tier 0: Implement Test Case
- **Description**: Add a new test function `finish_sets_finished_at` to the `tests` module in `src/services/jobs.rs`.
- **Expected Files**: `src/services/jobs.rs`
- **Verification**: Run `cargo test src/services/jobs.rs --lib`

### Tier 1: Verification
- **Description**: Execute the test suite to confirm the new test passes and no regressions were introduced.
- **Expected Files**: N/A
- **Verification**: Successful exit code from `cargo test`.

## Risk Assessment
- **Risk**: Low. The change is additive to a test suite.
- **Contract Surfaces**: None (test only).
