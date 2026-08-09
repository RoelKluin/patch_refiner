# patch_refiner — Roadmap

**Type:** Technical/engineering roadmap for a library, not a customer-facing product.
**Owner:** single maintainer (Roelof J.C. Kluin).
**Last verified against:** `patch_refiner_info.md` (CLAUDE.md), cross-checked against
`PATCH_REFINER_README.md`. Not a source read — see Assumptions.
**Detail lives in:** `PATCH_REFINER_ROADMAP_BACKLOG.md` (item-by-item status,
evidence, and exactly what still needs verifying against the real source tree).

---

## Vision

A dependable, sandboxed, library-first Rust crate that evaluates and refines
AI-generated patch candidates for automated-program-repair pipelines — usable
standalone and as a `ruchat` dependency, without either consumer's version
constraints or CLI dependencies leaking into the other.

## Assumptions — read this before trusting any status below

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

- Whether `clap`/CLI dependencies are actually feature-gated yet (Cargo.toml has to be
  read; documentation doesn't say).
- Whether `anyhow` is still in the public API, or already replaced.
- Whether Mode 3's error handling and the approved-path diagnostics bug (both flagged
  by the earlier independent code audit) are still present — the *cause* originally
  named (a stubbed `CompileChecker`) is gone, which doesn't by itself confirm the
  *symptom* is gone.

---

## Now

**Theme: close three critical items blocking the Next-tier boundary work.**

1. **Subprocess sandboxing** (critical) — currently no RLIMIT_*, no process group kill,
   no temp-dir isolation. This is a DoS vector once real model output reaches the
   checkers. Implement using verified external crates (`rlimit`, `nix`, `tempfile`,
   `shlex`) rather than hand-rolling. See `PATCH_REFINER_SANDBOXING_STRATEGY.md` for
   the full implementation sketch and comparison with home-grown approaches. **Effort:**
   ~2 hours. **Risk:** none — pure addition.

2. **Feature-gate clap** (blocking library consumers) — no `[features]` table; clap is
   a hard dependency. Blocks `cargo build --no-default-features`. **Effort:** ~30
   minutes.

3. **Replace anyhow with thiserror** (blocking ruchat integration) — callers need
   structured error matching, not string-based anyhow errors. **Effort:** ~2 hours.

All three are unblocked and independent; parallelize if possible.

## Next

**Theme: boundary + shadow mode with `ruchat`,** contingent on the Now theme being
closed. Both independent reviews of this crate (the code audit and the integration
review) converge on the same shape closely enough that it doesn't need re-deriving
here: draw a `refine`/`apply`-style boundary (structured facts out, no `diffy`/`anyhow`
types across the public API), add it as a path dependency, run it alongside `ruchat`'s
existing patch-validation logic without letting it decide anything, and log every
divergence.

- **Milestone:** shadow mode run against ≥50 real patch attempts spanning ≥3 models,
  zero unexplained divergence, and every "both accepted, different output" case at
  zero. This is the actual release gate — not a date.
- Repair logic must not derive rejections from model-written `@@` hunk offsets (a
  documented `ruchat` regression — see the intelligence-transfer file). State this as
  a tested invariant in this crate's own docs, not just as inherited tribal knowledge.

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

---

## Dependencies

- Next depends on Now being closed — don't add `ruchat` as a shadow-mode dependency
  while checker sandboxing is unresolved.
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
