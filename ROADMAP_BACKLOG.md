# patch_refiner — Roadmap Backlog (execution detail)

Companion to `PATCH_REFINER_ROADMAP.md`. Every item below is tagged with its evidence
and confidence level — this file exists specifically so status claims don't get
flattened into false certainty. Nothing here is a source read; "confirmed" means
"stated directly in `patch_refiner_info.md`/`CLAUDE.md`," not "verified against
`core.rs`" (though some items now include source verification).

---

## 1. Confirmed resolved — do not re-plan these

Each of these was the headline bug list in `old_patch_refiner_roadmap.md` and every
comments*.md merge attempt. `patch_refiner_info.md`/`CLAUDE.md` describes them as done:

| Item | Evidence | Prior roadmap that flagged it |
|---|---|---|
| `src/lib.rs` exists, exports `models`/`core` | Verified against source | migration_roadmap.md Phase 1, comments1–4 |
| `clap` is feature-gated, optional for library consumers | Verified against `Cargo.toml`: `[features]` table with `cli`; `required-features = ["cli"]` on the bin | old_patch_refiner_roadmap.md |
| Config validation exists as a pattern | `LanguageWeights`/`WhitespaceConfig` each carry `validate()`, documented as the pattern | PATCH_REFINER_ROADMAP.md (superseded) §2.3 |
| Mode resolution implemented + tested | `resolve_mode` has a dedicated unit test, `test_resolve_mode` | — |
| A test suite exists | Inline `#[cfg(test)]` modules under `src/*.rs`, run via `cargo test` | "no tests exist" no longer accurate |
| README/docs exist locally | `PATCH_REFINER_README.md` (full 4-mode spec), `docs/DIFF_FORMAT.md` referenced | migration_roadmap.md's GitHub read found none of these |
| Library/CLI separation exists | Cargo.toml has a `[features]` table with `cli` feature | Needed for ruchat integration |
| `thiserror` replaces `anyhow` in library API | Verified against source: `core::RefineError` is `thiserror`-based, public API does not leak `anyhow` | Required by ruchat |
| `schema_version` is wired | Request accepts optional, response always sets `SCHEMA_VERSION`, `evaluate()` rejects mismatches | Integration requirement |
| Per-language default commands wiring | Verified against source: `main.rs` now actually calls `default_commands()` and applies the result to `compile_command`/`test_command` when unset | Previously had a `let _default_commands` that was discarded |
| Diagnostics included in all responses | Verified against source: `RefinementResponse.diagnostics` is populated in every return path, including `Decision::Approved` | Integration requirement |

---

## 2. Intentionally out of scope — architectural decision, not unfinished work

**Semantic checks (compile, test, lint) are delegated to `ruchat`.** This is not a
stub, regression, or "later" item — it's an architectural decision confirmed across
all three source files (README.md, CLAUDE.md, ROADMAP.md).

| Item | Why out of scope | Evidence |
|---|---|---|
| `CompileChecker`/`TestChecker` subprocess execution in patch_refiner | Subprocess sandboxing is implemented once, in `ruchat`, using external crates (`rlimit`, `nix`, `tempfile`, `shlex`). Duplicating this in patch_refiner wastes effort and couples both repos to the same sandboxing impl. | ROADMAP.md Assumptions; CLAUDE.md Architectural decision section |
| `SemanticChecksConfig` fields being consumed by evaluator | Currently accepted (config still validates shape to keep schema stable for callers already sending these fields), but intentionally ignored by the evaluator. Config still passes through JSON for reference, but `run_compile_check`/`run_tests` do not trigger any subprocess execution. | CLAUDE.md: "Config fields like `SemanticChecksConfig`... are currently accepted but ignored by the evaluator. This is a deliberate design choice." |
| Diagnostic categories `Compile`/`Test` being emitted | No code path in patch_refiner emits these diagnostics. They are reserved schema values for potential future use or reference. Mode 3's `approved` decision means only: "syntactic validation passed; you may now proceed to semantic checks." | Source verification: no `push(Diagnostic { ... category: DiagnosticCategory::Compile ...})` in core.rs |
| Subprocess sandboxing (DoS guards, resource limits, temp cleanup) | Moved entirely to `ruchat` — patch_refiner has no subprocess-execution surface, so no sandboxing is needed. | ROADMAP_BACKLOG.md row 3.1; CLAUDE.md §Architectural decision |

**Future decision pending:** whether to remove `SemanticChecksConfig` fields from the
schema entirely (breaking change, cleaner schema surface) or keep them as stable
pass-through metadata for callers already sending them. See ROADMAP.md Decisions section.

---

## 3. Still open, or status genuinely unverified

### 3.1 Multi-file support — confirmed still absent, re-scoped (not re-prioritized down to zero)
`original_code: String` / single-file `apply(original, &patch)` is still the
described shape. Re-scoped per current `ruchat` intelligence
(`RUCHAT_ROADMAP_BACKLOG.md` §2): `ruchat` sends single-file patches sequentially (up
to 3/round) by design, so this is no longer a blocker for the `ruchat` integration
specifically. Still a real limitation for patch_refiner as a standalone evaluator —
keep on the roadmap, just behind the boundary/shadow-mode work rather than gating it.

### 3.2 Apply-failure diagnostics — open, high priority (Now tier)
When a patch fails to apply, patch_refiner currently returns a bare diffy error
(e.g., "hunk 1 does not match"). Should include the real current file content at the
apply-failure point (capped at ~4000 chars, per ruchat precedent) in the diagnostic
message. This is Mode 3's core value proposition — let callers understand why apply
failed without re-reading the original code. Evidence: `INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` §A.1 and traces from ruchat show this was a major improvement in the tool experience.
**Estimated:** ~1 hour. **Risk:** none — diagnostic enhancement.

### 3.3 Diff repair logic — open, high priority (Now tier)
Pre-parse repairs (before `diffy::Patch::from_str`) for documented AI-diff pathologies:
- Missing leading space on unchanged hunk context lines (automatically added back)
- Wrong `@@` hunk-header line counts (recomputed from hunk body)

Evidence: `INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` §A.1 lists these as the top two
pathologies ruchat's Worker produces; `§B.1` tells you where to find ruchat's working
implementations to port. **Estimated:** ~2 hours (porting + testing). **Risk:** low —
repairs must be conservative (don't change semantics), tested via corpus.

### 3.4 Anti-pattern invariant documentation — open, low effort (Now tier)
The invariant "never reject based on model-written `@@` offsets" should be written
into this repo's own docs (CLAUDE.md, ROADMAP.md, inline code comments) as a stated
design principle, not just inherited from ruchat's roadmap. Evidence: `INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` §A.2 documents a real regression (`commits 7998764` / `3a05df1`) where this invariant was violated, causing two full runs to fail.
**Estimated:** ~30 minutes. **Risk:** none — documentation/comment-only.

### 3.5 Mode naming / API ergonomics — open, low priority
`patch_refiner_info.md` itself still uses "Mode1/Mode2/Mode3" as live names in
`resolve_mode`'s description — the semantic-rename suggestion
(`ExemplarWithReason`, etc.) hasn't happened. Documentation/ergonomics win, not a
correctness issue; sequence as a deliberate, versioned, breaking change once the
public API is otherwise stable.

### 3.6 Code quality: duplication reduction — open, low priority
`cargo dupes` identified 5.5% exact duplication across 687 lines. ~60% of these are
load-bearing (especially the `ChangeSet::handle_part_inner` hypothesis-tracking
state machine — **do not touch**), but ~40% are genuine boilerplate that's safe to
consolidate: data-driven refactoring for `default_commands` (12 lines), config-merge
boilerplate (9 lines). Estimated 1–2 hours, zero breaking risk, all extractable via
pure-function refactoring. **Verified open, low priority** — safe to defer behind Now/Next items.

### 3.7 Repo hygiene — needs re-check against the actual remote
`migration_roadmap.md`'s GitHub read (no README/LICENSE/tests/CI, 1 commit) is known
to be behind local state. Before either repo pins a dependency on the other: confirm
the local work described in `patch_refiner_info.md` is **actually pushed**, and that
CI exists and triggers on the correct default branch (see `PATCH_REFINER_ROADMAP.md`
Dependencies section — this exact footgun is a live, named issue in `ruchat` and
shouldn't be re-imported here).

## 4. Deferred by design — sequencing rationale, not a "someday" dump

Every prior review (the code audit, the integration review, and all four merge
attempts) independently converges on gating these behind a proven shadow-mode zero
false-repair rate:

- Fuzzy/GNU-style context matching — `diffy` deliberately omits GNU patch's fuzzy
  matching; likely the single largest current source of refusals for otherwise-correct
  diffs, but risky enough to require the metric first.
- `git apply` as an alternate engine, behind a non-default feature so the core stays
  pure/sync.
- Anchor-based hunk relocation (ignore `@@` line numbers when context is unique) —
  directly related to the anti-pattern lesson (§A.2 in the intelligence-
  transfer file): relocation logic must be content-anchored, never offset-derived.
- Indentation/whitespace renormalization — flagged by every review as the riskiest
  repair class; ship last, off by default.
- `compute_distance`'s comment/raw-string-aware lexer is described as fragile-but-
  deliberate, with documented invariants and an explicit note not to simplify it
  without re-running the full suite. Treat as "handle with care," not a roadmap item,
  unless a specific bug surfaces.

## 5. Recommended immediate sequence

- Complete the Now tier (§3.2, §3.3, §3.4) in any order — all are syntactic-validation
  improvements, unblocked by each other, low risk.
- Then proceed to the boundary-drawing / shadow-mode work in `PATCH_REFINER_ROADMAP.md`'s
  Next tier, informed by the anti-pattern invariant and the repair logic now in place.
