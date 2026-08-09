# compute_distance / ChangeSet lexer: handle with care

This is a hand-rolled single-pass lexer classifying diff tokens into
code/string/comment regions (Rust raw strings, `//`, `/* */`). It has
gone through multiple rounds of subtle regressions. Do not simplify,
"clean up," or refactor it without re-running the full test suite,
including the deliberately-ambiguous-input tests.

Known failure modes, each previously caused a real regression:
- Multi-char markers (`//`, `/*`, `*/`, quote+hash sequences) can arrive
  as separate single-char tokens. Buffered symbolic tokens must not be
  matched against both an open and a close marker at once — that causes
  premature commits (e.g. a plain string starting `"##...` misread as a
  raw-string close).
- A bare `"` encountered while already inside a plain string must
  resolve immediately, not buffer. Buffering it breaks empty string
  literals (`""`).
- Ambiguous markers are recorded as alternate hypotheses and retried.
  When adding a new hypothesis, dedup against the stored/normalized
  representation, not the raw token — deduping on the wrong key causes
  duplicate hypotheses and combinatorial blowup.
- The outer hypothesis search has no loop guard beyond exhausting the
  pair space. Any change here must be tested against deliberately
  ambiguous strings/comments to confirm it still terminates, not just
  that it gives the right answer on clean input.
- The `eprintln!`/`println!` calls in this area are intentional debug
  output (visible with test `--nocapture`), not leftover debugging code
  to delete.
