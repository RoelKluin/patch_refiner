# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`patch_refiner` (crate name `patch_refiner`, binary `patch-refiner`) is a Rust library + CLI that evaluates AI-generated patch candidates for automated program repair (APR) pipelines. It reads a JSON request (original code + candidate diffs + optional "perfect" reference patches + config) and writes a structured JSON `RefinementResponse` describing whether a candidate is `approved`, `rejected`, or `failed`, with diagnostics and distance scoring.

Full behavioral spec (modes, config schema, CLI flags, diagnostic categories) is in `README.md` — read it before changing evaluation logic, since the modes are the core contract of this tool. `docs/DIFF_FORMAT.md` documents the unified-diff format assumptions (via the `diffy` crate). `ROADMAP.md` tracks known gaps/incomplete work (e.g. semantic checkers were originally stubs).

## Commands

```bash
cargo build --release          # build the release binary at ./target/release/patch-refiner
cargo test                     # run all tests (unit tests live inline in src/*.rs under #[cfg(test)])
cargo test <test_name>         # run a single test, e.g. cargo test test_resolve_mode
cargo test -- --nocapture      # see eprintln!/println! debug output from tests (core.rs uses these for diagnostics)
cargo run -- --input request.json > response.json
cat request.json | cargo run -- --mode mode3 --compile-check
```

There is no separate lint/format CI config in this repo beyond standard `cargo fmt` / `cargo clippy`.

## Architecture

Four files in `src/`, wired together via `src/lib.rs` (`pub mod models; pub mod checkers; pub mod core;`):

- **`models.rs`** — the authoritative data model and JSON schema (`RefinementRequest`, `RefinementResponse`, `PatchCandidate`, `PerfectPatch`, `RefinementConfig`, `Diagnostic`, etc.). Any change to the wire format starts here. Config sub-structs (`SemanticChecksConfig`, `LanguageWeights`, `WhitespaceConfig`) each carry their own `validate()` method — new config fields should follow that pattern.
- **`core.rs`** — `PatchRefiner::evaluate()` is the entry point. It:
  1. Validates config, resolves the `ApplicationMode` (`resolve_mode`: explicit `mode_override`, else inferred from the presence/count/reasoning of `perfect_patches` — 0 perfects → Mode3, 1 with `reason` → Mode1, 1 without → Mode2, >1 → Mode4).
  2. Mode3 (no reference patches) → `evaluate_mode_3`: applies each candidate diff and runs it through the `checkers` pipeline (compile/test), returning the first candidate that passes all checks.
  3. Modes 1/2/4 (reference patches known) → `evaluate_modes_1_2_4`: applies each candidate and each perfect patch to `original_code`, normalizes both (via `WhitespaceConfig`), and does exact-match comparison. On no exact match, computes a weighted token-level distance (`compute_distance`, using `prettydiff::diff_words` + `LanguageWeights`) to find and report the closest perfect patch as a `Deviation`.
  - The bulk of `core.rs` beyond the mode logic is a hand-rolled lexer/tokenizer (`StringExt` trait, `Run`, `ChangeSet`) that classifies diff tokens into code/string/comment regions (aware of Rust raw strings `r#"..."#`, `//` and `/*...*/` comments) so `compute_distance` can weight them differently via `LanguageWeights`. This is intricate, stateful, single-pass logic — read `handle_part`/`handle_part_inner` closely before touching it, and note the module currently has debug `eprintln!`/`println!` calls left in (`finalize`, `compute_distance`) that are not gated behind a logging flag.
- **`checkers.rs`** — `SemanticChecker` trait with `CompileChecker` and `TestChecker` impls, both delegating to `execute_command_with_timeout` → `execute_command`, which spawns `cmd.split_whitespace()`-parsed subprocesses via `std::process::Command` with a `wait-timeout`-enforced deadline. Commands are naive-split (no shell, no quoting support) — see `ROADMAP.md` §5.3 for known subprocess-safety caveats before changing this.
- **`main.rs`** — `clap`-based CLI. Reads JSON from `--input <FILE>` or stdin, applies CLI flag overrides onto the parsed `RefinementConfig` (`--mode`, `--compile-check`, `--test-check`, `--ignore-whitespace`), calls `PatchRefiner::evaluate`, prints pretty JSON to stdout. Note: `default_commands()` (per-language default compile/test commands) is defined here but its result is currently discarded (`let _default_commands = ...`) — see the `FIXME` above it.

### Key invariants when modifying evaluation logic

- `RefinementResponse.schema_version` must stay in sync with `models::SCHEMA_VERSION`.
- Mode resolution (`resolve_mode`) is order-sensitive and has a dedicated unit test (`test_resolve_mode` in `core.rs`) — extend it when adding new inference rules.
- `compute_distance` treats lower scores as closer/better matches (`distance_score`, "lower is better" per README).
- Patch parsing/application always goes through `diffy::Patch::from_str` / `diffy::apply`; unsupported diff shapes (binary, multi-file, rename-only) are out of scope per `docs/DIFF_FORMAT.md`.
