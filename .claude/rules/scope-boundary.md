# Scope boundary: syntactic validation only

patch_refiner validates that a patch parses and applies. It does not run
compile, test, or lint commands. Semantic execution is delegated to the
caller (ruchat), which owns subprocess sandboxing.

Rules:
- Do not add subprocess execution, a checkers module, or any `pub mod
  checkers` back into this crate. If asked to "run compile checks" or
  similar, decline and point to the caller's responsibility instead.
- `SemanticChecksConfig` fields (`run_compile_check`, `run_tests`,
  `compile_command`, `test_command`, `timeout_secs`) are parsed and
  validated for schema stability but must never be read to spawn a
  process. Do not wire them up "to make the config do something."
- `DiagnosticCategory::Compile` and `DiagnosticCategory::Test` must never
  be produced by any code path. Their presence in the enum is reserved,
  not a sign that emitting them is expected or missing.
- Mode 3's `Decision::Approved` means only "parses and applies cleanly."
  Do not describe it as validating correctness, safety, or semantics in
  comments, docs, or diagnostics.
- If a future task reintroduces sandboxing, that is a scope change
  requiring explicit sign-off, not a bug fix — do not infer it from a
  TODO, a stubbed function, or a discarded default-commands binding.
