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

### 2.1 CLI/library separation — feature-gating unverified
`main.rs` does `clap`-based CLI parsing layered onto `RefinementConfig`. Nothing
confirms `clap` is behind an optional `cli` feature (the original verified
`Cargo.toml` had it as a hard dependency). **Verify:** `Cargo.toml` for a
`[features]` table with `clap` marked `optional = true`.

### 2.2 `anyhow` in the public API — unverified, likely still present
Verified as a dependency originally; nothing mentions a `thiserror` migration.
Non-negotiable before `ruchat` depends on this crate per both independent reviews —
`ruchat`'s own code-review standard (`ruchat_info.md`: "prefer `thiserror` + `#[from]`
and `anyhow` context") independently confirms this isn't a one-sided ask. **Status:
open**, sequence before the Next-tier boundary work.

### 2.3 Diagnostics dropped on success / Mode 3 error handling — unverified
The specific *cause* named in the old audit (stubbed `CompileChecker`) is gone, which
doesn't confirm the *symptom* (diagnostics dropped, Mode 3 swallowing parse/apply
errors for malformed candidates) is fixed. **Verify against `core.rs`
`evaluate_modes_1_2_4` and `evaluate_mode_3` directly.**

### 2.4 Per-language default commands built but not wired — confirmed open
`patch_refiner_info.md` states this plainly: `main.rs::default_commands()`'s result is
discarded (`let _default_commands = ...`), FIXME above it. The one item in this
document confirmed open by an explicit code marker rather than inference.

### 2.5 Subprocess sandboxing — confirmed open, confirmed now-urgent
`patch_refiner_info.md`: "No shell, no quoting support - see ROADMAP §5.3 before
changing." Same concern as `PATCH_REFINER_ROADMAP.md` (superseded) §5.3 — shell
injection risk, no restricted subprocess environment, no memory/CPU limits. Was lower
priority when checkers were stubs that never executed; now higher priority, because
they run real commands against real (occasionally hallucination-adversarial) model
output. `ruchat`'s `orchestrator::cargo::limit_resources`
(`RLIMIT_AS`/`RLIMIT_CPU` + wall-clock timeouts) is a working, tested pattern to port
— see `INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` §A.4.

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

### 2.8 `schema_version` — partially addressed, runtime enforcement unverified
Stated as a developer invariant ("`RefinementResponse.schema_version` must stay in
sync with `models::SCHEMA_VERSION`") with presumed test coverage. Whether a mismatched
incoming `schema_version` is actually *rejected at parse/evaluate time* is unstated.
**Verify.**

### 2.9 CLI one-way flags / invalid-mode handling — unverified
Original bug: `--mode garbage` silently clobbers a valid JSON `mode_override`;
`--ignore-whitespace` etc. are one-way overrides with no way to disable something the
JSON enabled. Not mentioned as fixed or broken in `patch_refiner_info.md`; `main.rs`'s
description is consistent with either the old behavior or a fixed
`Option<bool>`-based one. **Verify before treating as closed.**

### 2.10 Repo hygiene — needs re-check against the actual remote
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
