# Patch application invariants

- Never derive a rejection, a diagnostic severity, or an apply-failure
  decision from a model-written `@@ -start,count +start,count @@` line
  count. These counts are unreliable; if they matter, recompute them from
  the hunk body instead. This caused a confirmed regression upstream
  (two full runs broke on this exact pattern before revert). Treat any
  code that compares reported vs. computed offsets to decide rejection
  as a bug, not a validation feature.
- Prefer repairing a recoverable diff over rejecting it. Malformed but
  recoverable shapes (e.g. a missing leading space on an unchanged
  context line) belong in a pre-parse repair step, not a rejection path.
  Only reject when the fix would require guessing intent (e.g. a missing
  `--- a/<file>` header).
- Out of scope by design, not a gap to fill silently: binary diffs,
  multi-file diffs in one candidate/perfect entry, and rename/move-only
  patches. If one of these appears, surface it as a diagnostic, don't
  attempt to parse or infer structure.
- `RefinementResponse.schema_version` and `models::SCHEMA_VERSION` must
  change together. A schema/field change without a version bump is a
  mistake, not a minor edit.
- `resolve_mode`'s inference order (mode_override, then perfect_patches
  count/reason) is order-sensitive and covered by a dedicated test. Any
  new inference rule must extend that test, not just add a new branch.
