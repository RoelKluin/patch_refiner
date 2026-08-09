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

## 2. Still open, or status genuinely unverified

### 2.1 CLI/library separation — **CONFIRMED OPEN**
**Verified against source:** `Cargo.toml` has no `[features]` table. `clap` is a hard
dependency on all builds, not optional. This blocks `cargo build --no-default-features`
(a requirement for library consumers who don't want a CLI parser in their dep tree).
**Status: remains open, sequenced before Next-tier boundary work.**

### 2.2 `anyhow` in the public API — **CONFIRMED OPEN**
**Verified against source:** `pub fn evaluate(req: RefinementRequest) -> Result<RefinementResponse>` uses `anyhow::Result`, not a custom error type. Error construction throughout uses `anyhow::anyhow!()` and `anyhow::bail!()`. Non-negotiable before `ruchat` depends on this crate per both independent reviews — `ruchat`'s own code-review standard independently confirms this isn't a one-sided ask. **Status: open, sequence before Next-tier boundary work.**

### 2.3 Diagnostics dropped on success / Mode 3 error handling — **FIXED**
**Verified against source:** `evaluate_modes_1_2_4` passes the accumulated
`diagnostics` vector through to the `RefinementResponse` on the approved path, and
`evaluate_mode_3` correctly records both parse and apply errors as `Error`-level
diagnostics before continuing to the next candidate. The old audit's concern was
based on an outdated snapshot. **Remove from backlog — not an open issue.**

### 2.4 Per-language default commands wiring — **MOSTLY FIXED, dead code cleanup only**
**Verified against source:** the first `let _default_commands = ...` binding (the one
with the FIXME) is dead code, but later in `main()` the function is called and
actually used to set defaults when no config value is provided. The feature itself
works; only the dead binding needs removing (1-line cleanup). Low priority.

### 2.5 Subprocess sandboxing — confirmed open, confirmed now-urgent
Current state: `cmd.split_whitespace()` parsing (fragile, no quoting support), no
RLIMIT_*, no process group kill, no temp-dir isolation, no environment filtering.
Shell injection risk, memory/CPU DoS possible, zombie processes on timeout. Was lower
priority when checkers were stubs; now critical because they run real commands
against occasionally hallucination-adversarial model output. **Recommended approach:**
use verified external crates (`rlimit` for resource limits, `nix` for process groups,
`tempfile` for scoped directories, `shlex` for safe command parsing) rather than
porting ruchat's code or hand-rolling. See `PATCH_REFINER_SANDBOXING_STRATEGY.md` for
implementation sketch and crate comparison. **Effort:** ~2 hours. **Risk:** none.

### 2.6 Multi-file support — confirmed still absent, re-scoped (not re-prioritized down to zero)
`original_code: String` / single-file `apply(original, &patch)` is still the
described shape. Re-scoped per current `ruchat` intelligence
(`RUCHAT_ROADMAP_BACKLOG.md` §2): `ruchat` sends single-file patches sequentially (up
to 3/round) by design, so this is no longer a blocker for the `ruchat` integration
specifically. Still a real limitation for patch_refiner as a standalone evaluator —
keep on the roadmap, just behind the boundary/shadow-mode work rather than gating it.

### 2.7 Mode naming / API ergonomics — open, low priority
`patch_refiner_info.md` itself still uses "Mode1/Mode2/Mode3" as live names in
`resolve_mode`'s description — the semantic-rename suggestion
(`ExemplarWithReason`, etc.) hasn't happened. Documentation/ergonomics win, not a
correctness issue; sequence as a deliberate, versioned, breaking change once the
public API is otherwise stable.

### 2.8 `schema_version` — **FIXED**
**Verified against source:** `evaluate()` uses `anyhow::ensure!(sv == SCHEMA_VERSION, ...)`
to reject a mismatched incoming `schema_version`. Not an open issue.

### 2.9 CLI one-way flags / invalid-mode handling — **FIXED**
**Verified against source:** CLI flags use `Option<bool>` (clap derive), so missing
flags leave the config unchanged — not one-way overrides. Mode parsing uses a proper
match with a catch-all that `anyhow::bail!("invalid --mode value: {other}")` on
unrecognized values. Both are fixed. Not an open issue.

### 2.10 Code quality: duplication reduction (verified, low priority)
`cargo dupes` identified 5.5% exact duplication across 687 lines. ~60% of these are
load-bearing (especially the `ChangeSet::handle_part_inner` hypothesis-tracking
state machine — **do not touch**), but ~40% are genuine boilerplate that's safe to
consolidate: CompileChecker/TestChecker duplication (38 lines), `default_commands`
data-driven refactor (12 lines), config-merge macro boilerplate (9 lines), and
`run_side` match-arm dispatch (8 members). Estimated 2–3 hours, zero breaking risk,
all extractable via pure-function refactoring. **Verified open, low priority** — safe
to defer behind Now/Next items. See `PATCH_REFINER_CODE_QUALITY.md` for concrete
refactoring sketches.

### 2.11 Repo hygiene — needs re-check against the actual remote
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

1. **Verify, don't assume** — read `core.rs`/`main.rs`/`Cargo.toml` directly to
   resolve §2.1–2.3, 2.8, 2.9. Half a day, unblocks everything downstream, and several
   items may already be fixed.
2. **Close §2.5** (subprocess sandboxing) before any shadow-mode wiring — highest
   actual-harm item now that checkers execute for real.
3. **Close §2.4** (default commands) — small, explicit, already has a FIXME pointing
   at it.
4. Proceed to the boundary-drawing / shadow-mode work in `PATCH_REFINER_ROADMAP.md`'s
   Next tier, informed by §2.6's corrected multi-file priority and the anti-pattern
   constraint from §3.

