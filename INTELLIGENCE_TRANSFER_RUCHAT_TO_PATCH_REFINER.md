# Intelligence transfer: ruchat → patch_refiner

**Document type: reference material, not a roadmap.** `RUCHAT_ROADMAP.md` and
`PATCH_REFINER_ROADMAP.md` both link here for supporting detail rather than repeating
it — this file has no themes, tiers, or milestones of its own; it's the evidence base
a couple of their items (particularly patch_refiner's Next tier and its §3 anti-pattern
note) depend on.

This is the "lessons learned about AI-derived patch refinement" document that was
requested in the original migration ask (`migration_roadmap_request.txt`) but never
produced. I don't have access to ruchat's actual working tree or its
`ruchat_traces/` directory — everything below is either (a) intelligence already
present in the documents you gave me, extracted and organized, or (b) precise
instructions for gathering the rest directly from the repo.

---

## Part A — Intelligence already available (no further gathering needed)

### A.1 The known diff pathologies ruchat's Worker actually produces
This is the closest thing to ground truth on "what do local models get wrong when
writing patches," confirmed via `RUCHAT_ORCHESTRATION.md`'s tool-catalog notes:

| Pathology | Current ruchat handling | patch_refiner implication |
|---|---|---|
| Missing leading space on unchanged hunk context lines | Repaired pre-parse (`normalize_diff_hunk_lines`) | Should be a `Repair` variant, applied before `diffy::Patch::from_str`, never rejected outright |
| Wrong `@@ -start,count +start,count @@` counts | Recomputed from hunk body (`fix_hunk_header_counts`) — bookkeeping only, body is source of truth | Same — a `Repair`, not a rejection. **Never validate against these counts as if the model's numbers were trustworthy** (see A.2) |
| Missing `--- a/<file>` header entirely | Not auto-inferred — actionable rejection telling the Worker to add one | Correctly out of scope to auto-fix; keep as a structured `RefineError` (`NoFileHeader`) with a message a caller can render as a retry instruction |
| Syntactically valid diff, hallucinated/stale context (doesn't match real file content) | Rejection includes the file's *actual current content*, capped at 4,000 chars, so the model can retry same-round without another tool call | This is the strongest single piece of intelligence here: **a bare "hunk did not match" error is not enough.** `ApplyFailure` needs the real content window, not just a diff error string, or the caller loses the one-shot self-correction ruchat currently relies on |
| Oversized diffs | Rejected before touching disk at 8,000 bytes/call (`MAX_PATCH_DIFF_BYTES`) | This is ruchat policy (`max_diff_bytes` in `RefineConfig`), not something patch_refiner should hardcode — but the config *shape* to carry it is already validated by real use |
| Multi-file diffs in one call | Never happens by design — up to 3 sequential single-file `apply_patch` calls per round instead | Patch_refiner does not need multi-file dispatch to serve ruchat (re-scopes prior roadmap assumptions — see `PATCH_REFINER_ROADMAP_CURRENT.md` §2.6) |

### A.2 A documented regression to avoid re-introducing
Commit `7998764` in ruchat added a guard comparing a diff's *computed* hunk-header
line offsets against a tool's *reported* line number, and used a mismatch to reject
otherwise-correct edits. It broke two full runs (traces 499 and 500) before being
caught and reverted in `3a05df1`. The lesson as recorded:

> Nothing that rejects an edit may be derived from model-written `@@` offsets — this
> repo already has to recompute them. Prefer repairing a recoverable diff over
> rejecting it, and prefer content-anchored checks over positional ones.

**This should be written into patch_refiner's own design docs as a stated invariant**,
not left as tribal knowledge in ruchat's roadmap. It directly bears on any diagnostic
or deviation logic that uses hunk line numbers.

### A.3 A negative result: don't re-litigate the apply mechanism
A `replace_in_file` (search-and-replace) tool was built as an alternative to
diff-based patching and reverted the same day — real runs showed no improvement over
`apply_patch`/`diffy`. If patch_refiner's design work ever considers an alternative
application strategy to unified diffs, this has already been tried once against real
traffic and didn't help. Worth citing, not repeating.

### A.4 Subprocess sandboxing: use verified external crates, not home-grown code
Both ruchat and patch_refiner need subprocess sandboxing. The Rust ecosystem has
audited, battle-tested crates for this (`rlimit`, `nix`, `tempfile`, `shlex`) that
are better than home-rolling or porting ruchat's pattern. Whether ruchat's own
`orchestrator::cargo::limit_resources` is already using these crates or is
hand-rolled, patch_refiner should adopt the external-crate stack as the preferred
approach for both projects. See `PATCH_REFINER_SANDBOXING_STRATEGY.md` for the full
implementation sketch, crate comparison, and effort estimates. This is a process
isolation concern that shouldn't vary between projects — standardize on the
ecosystem solution.

### A.5 ruchat's own trace/lessons infrastructure is directly reusable as the corpus source
This is not a coincidence worth losing: `RUCHAT_ORCHESTRATION.md` describes ruchat as
already producing exactly the artifact patch_refiner's Phase 5 (shared corpus) wants —
per-run trace files, round-by-round `GOOD:`/`BAD:`/`UNCLEAR:` verdicts, and up to three
`LESSON:` lines per run designed specifically to be grepped across runs for recurring
patterns. Part B below is just "point the existing collection mechanism at
patch_refiner's corpus format" rather than building new tooling.

---

## Part B — Instructions for gathering the rest directly from the repo

Everything here needs to be run against the actual ruchat working tree, which I don't
have access to.

### B.1 Where to look, and for what

| Location | What's there | Extraction target |
|---|---|---|
| `ruchat_traces/failures/*.md` | Full round-by-round trace + a "why it failed" summary — the richest source, per `Context::finalize_failure_trace` | Raw diffs the model attempted, the rejection reason, and the real file content shown back to the model |
| `ruchat_traces/successes/*.md` | Short summary only, no full trace (deliberately — see `RUCHAT_ORCHESTRATION.md`) | Lower value for corpus-building; useful for confirming what a *correct* patch looked like for a given goal |
| `ruchat_traces/summaries/*.md` | Round-by-round `GOOD:`/`BAD:`/`UNCLEAR:` verdicts plus up to 3 `LESSON:` lines per run, written by `Context::finalize_summary_trace` | The fastest path to recurring patterns — this is exactly the pre-digested "lessons learned" you asked for |
| `TODO.md` section 0 | Live, root-caused list of the 14 contributors to the current ~99/100 failure rate | Cross-reference against the corpus — which contributors are actually diff/patch-shaped vs. task-decomposition-shaped (only the former is patch_refiner's problem) |
| `git log` around commits `7998764` / `3a05df1` | The offset-guard regression and its revert (see A.2) | Confirm the exact diff/commit content if a fuller writeup is wanted than the roadmap summary gives |
| `agent/protocol.rs`, `agent/diff_repair.rs` (or wherever it now lives — the migration docs' names may be stale) | The actual `normalize_diff_hunk_lines`/`fix_hunk_header_counts`/multi-header-detection implementations referenced throughout | Source to port into patch_refiner's `refine()`, not just the behavioral description in A.1 |

**Note on a small discrepancy in the source docs:** `ruchat_info.md` gives this grep
command for finding lessons —
```
grep -h '^LESSON:' ruchat_traces/{successes,failures}/*.md | sort | uniq -c | sort -rn
```
— but `RUCHAT_ORCHESTRATION.md` describes `LESSON:` lines as written specifically into
`ruchat_traces/summaries/`, with `successes/` and `failures/` holding different content
(a short summary, and summary+full-trace, respectively). **Run the grep against all
three directories** to be safe; confirm which one(s) actually contain `LESSON:` lines
once you're in the repo, and correct whichever doc is stale.

### B.2 Extraction procedure

For each `ruchat_traces/failures/*.md` (and any success case worth keeping as a
positive example):

1. Identify every `apply_patch` tool call in the trace and its outcome (applied /
   rejected-parse / rejected-apply / rejected-scope / rejected-size).
2. For each rejected-apply case, pull:
   - the raw diff the Worker submitted (`raw.diff`)
   - the file's actual content at that point (`original.txt`) — the trace already
     contains this, since it's what was shown back to the model
   - the rejection category and message
3. Write these into patch_refiner's existing corpus location and shape (already
   specified in the migration plans, and consistent with `PATCH_REFINER_README.md`'s
   diagnostic categories):
   ```
   patch_refiner/tests/corpus/<case-id>/
     raw.diff
     original.txt
     expected.json     # the RefinementResponse you'd want back — decision,
                        # diagnostics (category from PATCH_REFINER_README.md's
                        # patch_parse / patch_apply / compile / test / similarity
                        # / other), and repairs applied if any
   ```
4. Record the model name and date per case in `expected.json` or a sibling metadata
   file — repair rules are model-behavior-dependent and will age out, per the existing
   roadmap's own caution.
5. Tag each case with which of `TODO.md` section 0's 14 contributors it illustrates,
   where applicable — this lets you separate "patch_refiner can fix this" cases from
   "this is a task-decomposition failure, out of scope" cases (per §0 of
   `RUCHAT_ROADMAP_CURRENT.md`: not every trace failure is a diff-quality problem).

### B.3 What to explicitly exclude from the corpus
Per A.1's multi-file row and `RUCHAT_ROADMAP_CURRENT.md` §1.2: don't manufacture
synthetic multi-file corpus cases from ruchat traces — they don't occur in ruchat's
actual usage pattern (sequential single-file calls), so a corpus case shaped that way
wouldn't reflect real traffic. If multi-file cases are wanted for patch_refiner as a
standalone evaluator, source them separately and label them as such.

### B.4 A quick sanity check once the corpus exists
`PATCH_REFINER_ROADMAP_CURRENT.md` §4 flags subprocess sandboxing (§2.5 there) as the
most urgent open patch_refiner item precisely because the checkers are no longer
stubs — they run real commands. Before running any extracted corpus case through a
compile/test checker, confirm sandboxing is in place; a corpus built from real
(occasionally adversarial-by-hallucination, not by intent) model output is exactly the
input that sandboxing gap is meant to guard against.
