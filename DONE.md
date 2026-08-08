All "confirmed resolved (documentation-based)" items are onfirmed directly against source - see §1.

## 1. Confirmed resolved

| Item | Evidence |
|---|---|
| `src/lib.rs` exists, exports `models`/`checkers`/`core` | `pub mod checkers; pub mod core; pub mod models;` |
| `CompileChecker`/`TestChecker` are real subprocess execution, not stubs | `checkers.rs`: `execute_command_with_timeout` → `execute_command`, spawns via `std::process::Command`, enforced by `wait_timeout::ChildExt` |
| `SemanticChecksConfig.timeout_secs` is respected | Passed through to `execute_command_with_timeout` on every checker call; defaults to 30s via `unwrap_or(30)` |
| Mode resolution implemented + tested | `resolve_mode` in `core.rs`, covered by `test_resolve_mode` |
| A test suite exists | Inline `#[cfg(test)]` modules in `core.rs`: `test_normalize_text_ignore_whitespace`, `test_resolve_mode`, `test_compute_distance`, `runaway_strings_in_diff` |
| README/docs exist locally | `README.md` (full 4-mode spec, config examples, CLI usage, JSON schema reference), `docs/DIFF_FORMAT.md` (unified-diff format + `diffy` limitations) |


