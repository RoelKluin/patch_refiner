# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with
code in this repository.

## What this is

`patch_refiner` (crate `patch_refiner`, binary `patch-refiner`) evaluates
AI-generated patch candidates for automated program repair (APR) pipelines. It
reads a JSON request (original code + candidate diffs + optional "perfect"
reference patches + config) and writes a structured JSON `RefinementResponse`
(`approved`/`rejected`/`failed`, diagnostics, distance scoring).

Full behavioral spec (modes, config schema, CLI flags, diagnostic categories)
is in `README.md` - read it before changing evaluation logic; the modes are the
core contract of this tool. `docs/DIFF_FORMAT.md` documents the unified-diff
assumptions (via `diffy`). `ROADMAP.md` tracks known gaps (e.g. semantic
checkers were originally stubs).

## Commands

```bash
cargo build --release          # release binary at
./target/release/patch-refiner
cargo test                     # all tests (inline #[cfg(test)] modules under
src/*.rs)
cargo test <test_name>         # single test while iterating, e.g. cargo test
test_resolve_mode
cargo test -- --nocapture      # see eprintln!/println! debug output (core.rs
uses these heavily)
cargo fmt && cargo clippy --all-targets   # no separate lint/CI config beyond
these
cargo run -- --input request.json > response.json
cat request.json | cargo run -- --mode mode3 --compile-check
```

## Architecture

Four files, wired via `src/lib.rs` (`pub mod models; pub mod checkers; pub mod
core;`):

- **`models.rs`** - authoritative JSON schema (`RefinementRequest`,
  `RefinementResponse`, `PatchCandidate`, `PerfectPatch`, `RefinementConfig`,
  `Diagnostic`, etc.). Any wire-format change starts here. Config sub-structs
  (`SemanticChecksConfig`, `LanguageWeights`, `WhitespaceConfig`) each carry a
  `validate()` method - follow that pattern for new config fields.
- **`core.rs`** - `PatchRefiner::evaluate()` is the entry point:

- Validates config, resolves `ApplicationMode` (`resolve_mode`: explicit
  `mode_override`, else inferred from `perfect_patches` - 0 -> Mode3, 1 with
  `reason` -> Mode1, 1 without -> Mode2, >1 -> Mode4).
- Mode3 -> `evaluate_mode_3`: applies each candidate, runs it through the
  `checkers` pipeline, returns the first candidate passing all checks.
- Modes 1/2/4 -> `evaluate_modes_1_2_4`: applies each candidate and each
  perfect patch to `original_code`, normalizes both (`WhitespaceConfig`),
  exact-match compares. On no match, computes a weighted token-level distance
  (`compute_distance`) to report the closest perfect patch as a `Deviation`.
  Lower `distance_score` = closer match.

- **`checkers.rs`** - `SemanticChecker` trait, `CompileChecker`/`TestChecker`
  impls, both via `execute_command_with_timeout` -> `execute_command`, spawning
  `cmd.split_whitespace()`-parsed subprocesses (`std::process::Command`) under a
  `wait-timeout` deadline. **No shell, no quoting support** - see ROADMAP $5.3
  before changing.
- **`main.rs`** - `clap` CLI. Reads `--input <FILE>` or stdin, layers CLI flags
  (`--mode`, `--compile-check`, `--test-check`, `--ignore-whitespace`) onto
  parsed `RefinementConfig`, calls `evaluate`, prints pretty JSON.
  **`default_commands()`** (per-language default compile/test commands) is
  defined but its result is currently discarded (`let _default_commands = ...`);
  see the `FIXME` above it before assuming language defaults are wired up.

### `compute_distance` - read before touching

The bulk of `core.rs` beyond mode dispatch is a hand-rolled single-pass lexer
(`StringExt` trait, `Run`, `ChangeSet`) classifying `prettydiff::diff_words`
tokens into code/string/comment regions - aware of Rust raw strings
(`r#"..."#`, arbitrary hash count) and `//`/`/*...*/` comments - so
`LanguageWeights` can score a one-word change in a comment differently from the
same change in code. This logic went through several rounds of subtle
regressions; don't simplify it without re-running the full suite.

Non-obvious invariants:

- Multi-char markers (`//`, `/*`, `*/`, quote+hash sequences) can arrive as
  separate single-char diff tokens. `handle_part`/`handle_first_part` buffer
  symbolic tokens until a marker is confirmed or ruled out. Do not match partial
  candidates against both open _and_ close marker text indiscriminately - that
  reintroduces premature commits (previously caused any plain string starting
  `"##...` to be misread as a raw-string close).
- A bare `"` while already inside a plain string (`section == "\""`) must
  resolve immediately, not buffer - otherwise empty string literals (`""`) never
  close.
- Ambiguous/unresolved markers are recorded as alternate `Run` hypotheses and
  retried. When pushing a new `Run`, dedup against the _stored_ representation,
  not the raw token - a mismatched dedup key causes duplicate hypotheses and
  combinatorial blowup in the outer search.
- The outer hypothesis search in `compute_distance` has no loop guard beyond
  exhausting the `(i, j)` pair space. Test any change here against deliberately
  ambiguous strings/comments (see `runaway_strings_in_diff`-style tests) to
  confirm termination, not just correctness.
- Debug `eprintln!`/`println!` calls in `finalize` and `compute_distance` are
  intentional (run tests with `--nocapture` to see them) but are not gated
  behind a logging flag - don't mistake them for leftover debugging cruft to
  delete.

### Key invariants when modifying evaluation logic

- `RefinementResponse.schema_version` must stay in sync with
  `models::SCHEMA_VERSION`.
- `resolve_mode` is order-sensitive and has a dedicated unit test
  (`test_resolve_mode`) - extend it when adding inference rules.
- Patch parsing/application always goes through `diffy::Patch::from_str` /
  `diffy::apply`; unsupported diff shapes (binary, multi-file, rename-only) are
  out of scope per `docs/DIFF_FORMAT.md`.
- Subprocess checkers must respect `SemanticChecksConfig.timeout_secs`; a new
  checker that can hang indefinitely is a regression.
