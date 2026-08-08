# patch_refiner - Roadmap

**Type:** Technical/engineering roadmap for a library, not a customer-facing product.
**Owner:** single maintainer (Roelof J.C. Kluin).
**Verified against:** `src/{lib,models,core,checkers,main}.rs`, `Cargo.toml`, `README.md`,
`docs/DIFF_FORMAT.md`, `CLAUDE.md` - direct source read, superseding the prior
documentation-only draft. Every status below cites the function or file it was
confirmed against;

**Revision note (2026-08-09):** This supersedes `PATCH_REFINER_ROADMAP.md` +
`PATCH_REFINER_ROADMAP_BACKLOG.md`. Two corrections to that draft:
1. All "confirmed resolved (documentation-based)" items are moved to DONE.md 
   (`WhitespaceConfig` validation) turned out to be only partially true; corrected
   below rather than carried forward.
2. **The multi-file assumption was wrong.** The prior draft re-scoped multi-file diff
   support to "not urgent" based on an assumption that `ruchat` sends only sequential
   single-file patches. That assumption is now confirmed incorrect: **`ruchat` does
   need multi-file diff support from this crate.** This moves multi-file support from
   §3 (deferred) to §2 (Next, blocking) - see the note there for what changes as a
   result.

---

## Vision

A dependable, sandboxed, library-first Rust crate that evaluates and refines
AI-generated patch candidates for automated-program-repair pipelines - usable
standalone and as a `ruchat` dependency, without either consumer's version
constraints or CLI dependencies leaking into the other.

## Status legend

- Confirmed open - verified directly against current source; the gap exists.

---

## 1. Partially resolved

Config validation exists as a *pattern* but is not uniformly applied - see $2.11,
a new item found via this source read. `SemanticChecksConfig` and `LanguageWeights`
each define `validate()`, but `WhitespaceConfig` does not, and **`evaluate()` only
ever calls `config.semantic_checks.validate()`** - `LanguageWeights::validate()` is
defined but never invoked anywhere in `core.rs`. The prior draft's blanket "config
validation exists as a pattern" is true of the code shape but not of runtime
enforcement; do not treat `LanguageWeights` values as validated until §2.11 is closed.

---

## 2. Open, ordered by risk

### 2.1 Subprocess sandboxing - highest actual-harm item
`checkers.rs::execute_command` parses commands via `cmd.split_whitespace()` and spawns
directly with `std::process::Command` - no shell, but also no quoting support (a
command needing a quoted argument with embedded spaces is silently mis-split, not
rejected), no environment filtering, and no memory/CPU limits (only `wait_timeout`
wall-clock enforcement). This was lower risk while checkers were stubs; they now run
real commands against real model output. `ruchat`'s
`orchestrator::cargo::limit_resources` (`RLIMIT_AS`/`RLIMIT_CPU` + wall-clock timeout)
is a working, tested pattern to port - see
`INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` §A.4.

### 2.2 Multi-file diff support - corrected priority, now blocking
`PatchCandidate.diff_content: String` and `RefinementRequest.original_code: String`
are still single-file shapes; `evaluate_modes_1_2_4`/`evaluate_mode_3` both call
`diffy::apply(original, &patch)` once per candidate. `docs/DIFF_FORMAT.md` documents
this as a known limitation ("multiple files in a single patch... behavior is
implementation-defined").

**This is now a blocker for the `ruchat` integration**, not a standalone-evaluator
nicety - see the revision note above. Concretely this means:
- The §3 (Next) boundary-drawing work cannot finalize its public API shape
  (`refine`/`apply`-style, structured facts only) without deciding the multi-file
  request/response shape first - designing that boundary and then reworking it for
  multi-file later is the more expensive order.
- Suggested shape to evaluate: `files: BTreeMap<PathBuf, String>` on both
  `RefinementRequest.original_code` and per-candidate/per-perfect-patch diff content,
  with `compute_distance` and diagnostics gaining a `file` dimension alongside the
  existing line/column one.
- Sequence this before §3, not after - re-flagging the dependency the prior draft got
  backwards.

### 2.3 Diagnostics dropped on the approved path (Modes 1/2/4)
`evaluate_modes_1_2_4`'s exact-match return constructs `diagnostics: vec![]` on the
approved path, discarding whatever was accumulated in the loop up to that point (e.g.
warnings from other perfect patches that failed to parse or apply). Should return the
accumulated `diagnostics` vector instead of a fresh empty one.

### 2.4 Mode 3 swallows parse/apply errors silently
`evaluate_mode_3`'s candidate loop is gated by
`if let Ok(patch) = Patch::from_str(...) { if let Ok(ai_result) = apply(...) { ... } }`,
a candidate that fails to parse or fails to apply produces **no diagnostic at all**,
unlike `evaluate_modes_1_2_4`, which pushes a `Diagnostic` on both failure paths. A
caller currently cannot distinguish "candidate was syntactically invalid" from
"candidate parsed fine but failed compile/test checks" in Mode 3's response.

### 2.5 `schema_version` accepted but never enforced
`RefinementRequest.schema_version: Option<String>` is parsed but never compared
against `models::SCHEMA_VERSION` anywhere in `evaluate()`. The response side is
correctly disciplined (every return path sets
`schema_version: crate::models::SCHEMA_VERSION.to_string()`), but a request declaring
an incompatible schema version is silently processed rather than rejected.

### 2.6 CLI mode flag silently clobbers a valid JSON `mode_override`
```rust
if let Some(m) = cli.mode {
    config.mode_override = match m.to_lowercase().as_str() {
        "mode1" => Some(ApplicationMode::Mode1),
        ...
        - => None,   // overwrites an existing valid JSON mode_override with None
    };
}
```
`main.rs`: an invalid `--mode` value unconditionally sets `config.mode_override =
None`, discarding a valid value the input JSON may have already set. Should either
leave `config.mode_override` untouched on an unrecognized flag value, or hard-error
via `anyhow::bail!`.

### 2.7 Boolean CLI flags are one-way overrides
`--compile-check`, `--test-check`, `--ignore-whitespace` can only force a setting
*on*; there's no way via CLI to disable something the input JSON enabled (`bool` flags
via `clap`, not `Option<bool>`). Confirmed present in `main.rs` as originally flagged.

### 2.8 `clap` is not feature-gated
`Cargo.toml` has no `[features]` table; `clap` is an unconditional dependency. A
library-only consumer (`ruchat`, once it depends on this crate) pulls in CLI parsing
transitively. Needs a `cli` feature gating `main.rs`'s dependency and `[[bin]]` target.

### 2.9 `anyhow` is in the public API
`core.rs`: `pub fn evaluate(req: RefinementRequest) -> Result<RefinementResponse>` uses
`anyhow::Result`; errors are constructed via `anyhow!` with no structured variant type.
Confirmed still present, no `thiserror` migration has happened. Non-negotiable before
`ruchat` depends on this crate per both independent reviews - `ruchat`'s own
code-review standard independently confirms this isn't a one-sided ask.

### 2.10 Per-language default commands computed but discarded
`main.rs::default_commands()` return value is bound to `_default_commands` and never
used - matches the `FIXME` directly above the call site. Small, well-scoped, real gap.

### 2.11 Config validation not uniformly wired up (new, found via this read)
- `WhitespaceConfig` has no `validate()` method at all, unlike its two sibling config
  structs.
- `LanguageWeights::validate()` exists (checks `code_weight > string_weight >=
  comment_weight`) but is **never called** - `evaluate_modes_1_2_4` does
  `config.language_weights.clone().unwrap_or_default()` with no validation step.
- Only `config.semantic_checks.validate()` is actually invoked, in `evaluate()`.

Fix: call `language_weights.validate()` alongside `semantic_checks.validate()` in
`evaluate()`, and either add `WhitespaceConfig::validate()` (even as a no-op returning
`Ok(())` for now, for consistency) or document why it's exempt.

---

## 3. Next - boundary + shadow mode with `ruchat`

Contingent on §2 being closed, **including the now-blocking §2.2 multi-file work**.
Draw a `refine`/`apply`-style boundary (structured facts out, no `diffy`/`anyhow`
types across the public API - depends on §2.9), add it as a path dependency, run it
alongside `ruchat`'s existing patch-validation logic without letting it decide
anything, log every divergence.

- **Milestone (the actual release gate, not a date):** shadow mode run against ≥50
  real patch attempts spanning ≥3 models, zero unexplained divergence, and every
  "both accepted, different output" case at zero.
- **Stated invariant, not tribal knowledge:** repair logic must not derive rejections
  from model-written `@@` hunk offsets. This is a documented `ruchat` regression
  (commit `7998764`, reverted in `3a05df1`) - see
  `INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` §A.2. Write this into this
  crate's own docs (e.g. a `## Design invariants` section in `README.md`) rather than
  leaving it only in the intelligence-transfer file.
- Corpus-driven testing (`tests/corpus/`), sourced from `ruchat`'s failure traces -
  see the intelligence-transfer file §B for the exact extraction procedure and
  directory shape.

---

## 4. Later - deferred by design, not urgent

Every prior review (code audit, integration review, all merge attempts) converges on
sequencing these after a proven zero-false-repair shadow-mode rate; nothing in this
source read changes that ordering:

- Fuzzy/GNU-style context matching - `diffy` deliberately omits GNU patch's fuzzy
  matching; likely the largest current source of refusals for otherwise-correct
  diffs, but gated behind the shadow-mode metric first.
- `git apply` as an alternate engine, behind a non-default feature so the core stays
  pure/sync.
- Anchor-based hunk relocation (ignore `@@` line numbers when context is unique) -
  must be content-anchored per the §3 invariant, never offset-derived.
- Indentation/whitespace renormalization - riskiest repair class per every review;
  ship last, off by default.
- Mode1–4 → semantic names (`ExemplarWithReason`, etc.) - confirmed still
  `ApplicationMode::Mode1..Mode4` in `models.rs`. Real ergonomics win, but a breaking
  API change; sequence once the public API is otherwise stable (i.e. after §2.9's
  error-type migration, so it's one breaking-change window instead of two).
- `compute_distance`'s comment/raw-string-aware lexer (`core.rs`) - went through
  several rounds of hardening (buffering vs. premature marker commit, empty-string
  literal edge case, hypothesis dedup-key correctness - see `CLAUDE.md`'s documented
  invariants for this function). Current known failure modes from that hardening pass
  are resolved; treat as "handle with care, re-run the full suite before touching,"
  not as an open roadmap item unless a new bug surfaces.

---

## Dependencies

- §3 depends on §2 being closed - don't add `ruchat` as a shadow-mode dependency
  while checker sandboxing (§2.1) or the public error type (§2.9) are unresolved, and
  don't finalize the boundary API shape before multi-file (§2.2) is decided.
- §3 also depends on `ruchat` actually producing the intelligence described in
  `INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` (a corpus of real failure traces,
  the anti-pattern list) - that work happens on the `ruchat` side, not here.
- The maintainer confirms this repo's local work is actually pushed to the remote, and
  that CI exists and triggers on the  correct default branch. `ruchat` has a named,
  live footgun here (`ci.yml` targeting `main` while the default branch is `master`) -
  the master branch is correct.

## Where the detail is

`INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` - diff pathologies `ruchat`'s
Worker produces, the offset-guard regression writeup, the sandboxing pattern to port,
and the corpus-extraction procedure for §3.
