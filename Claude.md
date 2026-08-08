# patch_refiner

AI Patch-Refinement Module for APR (Automated Program Repair). Rust CLI/library
that scores or validates candidate patches against perfect patches (Mode 1/2/4,
similarity-based) or against compile/test checks (Mode 3).

## Commands

- Build: `cargo build`
- Test: `cargo test` (prefer `cargo test <test_name>` for a single test while
iterating)
- Run: `cargo run -- --input request.json` or pipe JSON via stdin
- Lint: `cargo clippy --all-targets`

## Architecture

- `src/models.rs` — JSON schema (`RefinementRequest`/`RefinementResponse`).
Treat this as the wire contract; changing field names/shapes breaks callers.
- `src/core.rs` — `PatchRefiner::evaluate()` dispatches on `ApplicationMode`
(resolved from config override, or from perfect-patch count/reason presence —
see `resolve_mode`).
- `src/checkers.rs` — `SemanticChecker` trait + subprocess-based
implementations (`CompileChecker`, `TestChecker`) used only in Mode 3.
- Modes 1/2/4 don't run subprocesses; they diff the candidate's applied result
against perfect patches via `compute_distance` in `core.rs`.

## `compute_distance` — read before touching

This is a hand-written state machine over `prettydiff::diff_words` tokens,
tracking Rust lexical context (`//`, `/*...*/`, `"..."`, `r#"..."#`-style raw
strings with arbitrary hash counts) so that a one-word change inside a comment
scores differently from the same change in code. It is intentionally more
complex than it looks; do not simplify without re-running the full test suite.

**Non-obvious invariants:**

- Multi-character markers (`//`, `/*`, `*/`, and quote+hash sequences) can
arrive as separate single-char diff tokens. `handle_part`/`handle_first_part`
buffer symbolic tokens until a marker is confirmed or ruled out — do not
shortcut this by matching partial candidates via `starts_with` against both
open _and_ close text indiscriminately; that reintroduces premature commits
(see git history for the `"###`-string regression).
- A bare `"` while `section == "\""` must resolve immediately (empty string
literals, e.g. `""`, must close cleanly) — this is the one section that must
never buffer.
- Ambiguous/unresolved markers are recorded as alternate `Run` hypotheses and
retried; when pushing a new `Run`, dedup against the _stored_ (possibly
reversed) representation, not the raw token — mismatched dedup keys cause
duplicate hypotheses and combinatorial blowup in `compute_distance`'s search
loop.
- `compute_distance` iterates `(i, j)` pairs over both sides' hypothesis lists
to a fixed point; there is no other loop guard. Any change here should be
tested against `runaway_strings_in_diff`-style inputs (deliberately ambiguous
strings/comments) to confirm termination, not just correctness.

## Gotchas

- `diffy::Patch::from_str` expects unified diff format; malformed candidate
diffs should produce a `Diagnostic` (category `PatchParse`), not a panic.
- Subprocess checkers must respect `SemanticChecksConfig.timeout_secs`; don't
add a checker that can hang indefinitely.
- No test corpus/fixtures directory yet — inline fixtures in `#[cfg(test)]`
modules are the current convention.


