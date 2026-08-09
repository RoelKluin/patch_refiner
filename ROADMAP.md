# patch_refiner — Roadmap

**Type:** Technical/engineering roadmap for a library, not a customer-facing product.
**Owner:** single maintainer (Roelof J.C. Kluin).
**Last verified against:** `patch_refiner_info.md` (CLAUDE.md), cross-checked against
`PATCH_REFINER_README.md`. Not a source read — see Assumptions.
**Detail lives in:** `PATCH_REFINER_ROADMAP_BACKLOG.md` (item-by-item status,
evidence, and exactly what still needs verifying against the real source tree).

---

## Vision

A dependable, library-first Rust crate that validates AI-generated patch candidates syntactically (parse and apply) for automated-program-repair pipelines — usable standalone and as a `ruchat` dependency, without either consumer's constraints leaking into the other.

**Semantic validation (compile, test, lint) is intentionally out of scope** and delegated to the calling system (`ruchat`), which implements a unified subprocess-sandboxing layer. This avoids duplicating complex isolation infrastructure.

## Assumptions — read this before trusting any status below

- **Architectural decision (confirmed, not just a roadmap item):** Subprocess
  sandboxing is implemented once, in `ruchat`, using verified external crates
  (`rlimit`, `nix`, `tempfile`, `shlex`). patch_refiner does not execute
  compile/test commands and has no sandboxing surface. See CLAUDE.md for the
  rationale. This means `SemanticChecksConfig` (config fields for compile/test)
  is currently accepted but ignored by the evaluator.

- **This roadmap is built from two CLAUDE.md-style summaries and a README, not a
  source read.** Every "confirmed resolved" claim in the backlog file is inference
  from documentation, not verification. Where the two available documents disagree
  or are silent, the backlog file says so explicitly.

- **The GitHub snapshot earlier roadmap drafts read (`migration_roadmap.md`) is
  known-stale** — 1 commit, no `[lib]`, no README/tests/CI — and contradicted by
  `patch_refiner_info.md`, which describes a working `src/lib.rs`, implemented
  checkers, and local docs. This is the confirmed "missing local commits" scenario:
  push the current local state before planning further against it.

- **`ruchat` does not need multi-file diff support from this crate.** Confirmed via
  current `ruchat` intelligence (see `RUCHAT_ROADMAP_BACKLOG.md` §2): it sends
  sequential single-file patches, not multi-file diffs. This re-scopes multi-file
  support from "blocking ceiling" to "useful for the standalone use case, not urgent."

## Decisions pending maintainer sign-off / verification

- Whether Mode 3's apply-failure diagnostics should include the real current file
  content (per `ruchat`'s ~4000-char precedent in the intelligence-transfer doc),
  rather than a bare diffy error string.
- Whether to remove `SemanticChecksConfig` fields from the schema (breaking change,
  cleaner), or keep them as pass-through metadata (stable, but potentially
  misleading). See ROADMAP_BACKLOG.md §2 for the full trade-off.

---

## Now

**Theme: repair logic for syntactic validation, and integration housekeeping.**

1. **Apply-failure diagnostics should include real file content** (high-value, Mode 3) —
   when a hunk doesn't match, patch_refiner currently returns a bare diffy error
   (e.g., "hunk 1 does not match"). Integrate the real current file content at the
   apply-failure point (capped at ~4000 chars, per ruchat precedent) into the
   diagnostic message. This is the single highest-leverage Mode 3 improvement — it
   lets a calling system (or a human reviewing the diagnostic) understand why the
   apply failed without re-reading the original code. Evidence: `INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` §A.1 identifies this as a documented improvement from ruchat's own tool traces.
   **Effort:** ~1 hour. **Risk:** none — pure diagnostic enhancement.

2. **Repair logic for common diff malformations** (Mode 3 robustness) — ruchat's
   experience shows AI-generated diffs consistently omit leading spaces on
   context lines and get `@@` hunk-header line counts wrong. Implement pre-parse
   repairs (before `diffy::Patch::from_str`) to fix these, rather than rejecting
   otherwise-correct diffs. This is Mode 3's core value proposition.
   See `INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` §A.1 for the pathologies
   and §B.1 for where to find ruchat's working implementations to port.
   **Effort:** ~2 hours (porting + testing against corpus cases). **Risk:** low —
   repairs must be conservative (don't change semantics), tested via corpus.

3. **Document the anti-pattern: never reject based on model-written `@@` offsets**
   (architectural invariant, CLAUDE.md/ROADMAP housekeeping) — add this as a
   stated design invariant in code comments and docs, not just in the roadmap.
   See `INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` §A.2 for the bug history.
   **Effort:** ~30 minutes. **Risk:** none — documentation only.

All three are unblocked and independent; parallelize if possible.

## Next

**Theme: boundary + shadow mode with `ruchat`,** contingent on the Now theme being
closed. Both independent reviews of this crate (the code audit and the integration
review) converge on the same shape closely enough that it doesn't need re-deriving
here: patch_refiner is now clearly scoped to syntactic validation (parse + apply),
and `ruchat` handles the semantic boundary. Add it as a path dependency, run it
alongside `ruchat`'s existing patch-validation logic without letting it decide
anything, and log every divergence.

- **Milestone:** shadow mode run against ≥50 real patch attempts spanning ≥3 models,
  zero unexplained divergence, and every "both accepted, different output" case at
  zero. This is the actual release gate — not a date.
- Integration point: patch_refiner's `Decision::Approved` in Mode 3 should map to
  ruchat's "safe syntactic precondition; proceed to semantic checks." Confirm this
  boundary is clear, testable, and doesn't regress.

## Later (deferred by design, not urgent)

- Multi-file support (`files: BTreeMap<PathBuf, String>`) — re-scoped from blocking to
  "worth doing for the standalone evaluator, not for `ruchat`."
- Fuzzy/GNU-style context matching, `git apply` as an alternate engine, anchor-based
  hunk relocation, indentation/whitespace renormalization — every prior review
  correctly sequences these after a proven zero false-repair rate; nothing here
  changes that.
- Mode1–4 → semantic names (`ExemplarWithReason`, etc.) — real ergonomics win, but a
  breaking API change best done once the public API is otherwise stable.
- Corpus-driven testing (`tests/corpus/`) sourced from `ruchat`'s own failure traces —
  see the intelligence-transfer file for exactly where to pull this from.
- `SemanticChecksConfig` removal or formalization — depends on the pending decision
  (see Decisions section).

---

## Dependencies

- Next depends on Now being closed — don't add `ruchat` as a shadow-mode dependency
  until the repair logic and anti-pattern documentation are in place.
- Next also depends on `ruchat` actually producing the intelligence described in
  `INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md` (a corpus of real failure traces,
  the anti-pattern list) — that work happens on the `ruchat` side, not here.
- Before either repo pins a dependency on the other: confirm patch_refiner's local
  work (README, docs, tests, `src/lib.rs`) is actually **pushed**, and that CI exists
  and triggers on the correct default branch — `ruchat` has a named, live footgun
  here (`ci.yml` targeting `main` while the default branch is `master`); don't import
  it.

## Where the detail is

`PATCH_REFINER_ROADMAP_BACKLOG.md` — the full confirmed-resolved / still-open /
unverified breakdown, with the specific evidence behind each status.
