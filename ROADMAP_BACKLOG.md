# patch_refiner — Roadmap Backlog (execution detail)

Companion to `PATCH_REFINER_ROADMAP.md`. Every item below is tagged with its evidence
and confidence level — this file exists specifically so status claims don't get
flattened into false certainty. Nothing here is a source read; "confirmed" means
"stated directly in `patch_refiner_info.md`," not "verified against `core.rs`."

---

## 1. Confirmed resolved — do not re-plan these

Each of these was the headline bug list in `old_patch_refiner_roadmap.md` and every
comments*.md merge attempt. `patch_refiner_info.md` describes them as done:

| Item | Evidence | Prior roadmap that flagged it |
|---|---|---|
| `src/lib.rs` exists, exports `models`/`checkers`/`core` | Stated directly | migration_roadmap.md Phase 1, comments1–4 |
| `CompileChecker`/`TestChecker` are real subprocess execution, not stubs | `checkers.rs` described as `execute_command_with_timeout` → `execute_command` via `wait-timeout` | old_patch_refiner_roadmap.md bug #3 — the single most dangerous item flagged (silent false-positive approval) |
| `SemanticChecksConfig.timeout_secs` is respected | Stated as a "key invariant": "a new checker that can hang indefinitely is a regression" | Previously listed as dead config |
| Config validation exists as a pattern | `SemanticChecksConfig`/`LanguageWeights`/`WhitespaceConfig` each carry `validate()`, documented as the pattern for new fields | PATCH_REFINER_ROADMAP.md (superseded) §2.3 |
| Mode resolution implemented + tested | `resolve_mode` has a dedicated unit test, `test_resolve_mode` | — |
| A test suite exists | Inline `#[cfg(test)]` modules under `src/*.rs`, run via `cargo test` | "no tests exist" no longer accurate |
| README/docs exist locally | `PATCH_REFINER_README.md` (full 4-mode spec), `docs/DIFF_FORMAT.md` referenced | migration_roadmap.md's GitHub read found none of these |
| CLI/Library separation exists | `Cargo.toml` has a `[features]` table with `cli` | Library consumers require --no-default-features |
| anyhow replaced by thiserror | `Cargo.toml` has a `[dependencies]` table with `thiserror` | required by ruchat |
|  Diagnostics dropped on success / Mode 3 error handling | Verified against source | audit |
| Per-language default commands wiring | Verified against source | |
| Subprocess sandboxing was removed / delegated to ruchat | Verified against source | |
| schema_version | Verified against source | |
| CLI one-way flags / invalid-mode handling | Verified against source | |


## 2. Still open, or status genuinely unverified

### 2.1 Multi-file support — confirmed still absent, re-scoped (not re-prioritized down to zero)
`original_code: String` / single-file `apply(original, &patch)` is still the
described shape. Re-scoped per current `ruchat` intelligence
(`RUCHAT_ROADMAP_BACKLOG.md` §2): `ruchat` sends single-file patches sequentially (up
to 3/round) by design, so this is no longer a blocker for the `ruchat` integration
specifically. Still a real limitation for patch_refiner as a standalone evaluator —
keep on the roadmap, just behind the boundary/shadow-mode work rather than gating it.

### 2.2 Mode naming / API ergonomics — open, low priority
`patch_refiner_info.md` itself still uses "Mode1/Mode2/Mode3" as live names in
`resolve_mode`'s description — the semantic-rename suggestion
(`ExemplarWithReason`, etc.) hasn't happened. Documentation/ergonomics win, not a
correctness issue; sequence as a deliberate, versioned, breaking change once the
public API is otherwise stable.

### 2.3 Code quality: duplication reduction (verified, low priority)
`cargo dupes` identified 5.5% exact duplication across 687 lines. ~60% of these are
load-bearing (especially the `ChangeSet::handle_part_inner` hypothesis-tracking
state machine — **do not touch**), but ~40% are genuine boilerplate that's safe to
consolidate: CompileChecker/TestChecker duplication (38 lines), `default_commands`
data-driven refactor (12 lines), config-merge macro boilerplate (9 lines), and
`run_side` match-arm dispatch (8 members). Estimated 2–3 hours, zero breaking risk,
all extractable via pure-function refactoring. **Verified open, low priority** — safe
to defer behind Now/Next items. See `PATCH_REFINER_CODE_QUALITY.md` for concrete
refactoring sketches.

### 2.4 Repo hygiene — needs re-check against the actual remote
`migration_roadmap.md`'s GitHub read (no README/LICENSE/tests/CI, 1 commit) is known
to be behind local state. Before either repo pins a dependency on the other: confirm
the local work described in `patch_refiner_info.md` is **actually pushed**, and that
CI exists and triggers on the correct default branch (see `PATCH_REFINER_ROADMAP.md`
Dependencies section — this exact footgun is a live, named issue in `ruchat` and
shouldn't be re-imported here).

## 3. Deferred by design — sequencing rationale, not a "someday" dump

Every prior review (the code audit, the integration review, and all four merge
attempts) independently converges on gating these behind a proven shadow-mode zero
false-repair rate:

- Fuzzy/GNU-style context matching — `diffy` deliberately omits GNU patch's fuzzy
  matching; likely the single largest current source of refusals for otherwise-correct
  diffs, but risky enough to require the metric first.
- `git apply` as an alternate engine, behind a non-default feature so the core stays
  pure/sync.
- Anchor-based hunk relocation (ignore `@@` line numbers when context is unique) —
  directly related to the `ruchat` anti-pattern lesson (§A.2 in the intelligence-
  transfer file): relocation logic must be content-anchored, never offset-derived.
- Indentation/whitespace renormalization — flagged by every review as the riskiest
  repair class; ship last, off by default.
- `compute_distance`'s comment/raw-string-aware lexer is described as fragile-but-
  deliberate, with documented invariants and an explicit note not to simplify it
  without re-running the full suite. Treat as "handle with care," not a roadmap item,
  unless a specific bug surfaces.

## 4. Recommended immediate sequence

-  Proceed to the boundary-drawing / shadow-mode work in `PATCH_REFINER_ROADMAP.md`'s
   Next tier, informed by §2.6's corrected multi-file priority and the anti-pattern
   constraint from §3.

