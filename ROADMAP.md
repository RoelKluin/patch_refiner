# patch_refiner Roadmap

**Project**: AI Patch Refinement Module for APR (Automated Program Repair)  
**Current Version**: 0.1.0  
**Status**: Alpha - Core logic complete, semantic checkers incomplete

---

## Phase 1: Critical Path (Blockers)

### 1.1 Implement Subprocess Execution (HIGH PRIORITY)
- **Issue**: `CompileChecker::check()` is stubbed; Mode 3 evaluation cannot run
- **Work**:
  - Implement subprocess spawning with `std::process::Command`
  - Handle timeout enforcement via `config.semantic_checks.timeout_secs`
  - Capture stdout/stderr and convert to `Diagnostic` objects
  - Parse compiler error output (language-specific: rustc, gcc, clang, javac, etc.)
- **Assumption**: `compile_command` in config is a shell-escaped string (e.g., `"cargo build"`)
- **Acceptance**: Mode 3 can select first passing candidate or return `Failed` decision

### 1.2 Implement Test Runner
- **Issue**: `SemanticChecksConfig.run_tests` flag exists but no checker implements it
- **Work**:
  - Create `TestChecker` similar to `CompileChecker`
  - Execute `config.semantic_checks.test_command` with timeout
  - Distinguish test pass/fail from test runner errors
  - Aggregate test failures into `Diagnostic` objects
- **Acceptance**: Mode 3 accepts patches that pass both compile + tests

---

## Phase 2: Correctness & Robustness

### 2.1 Error Handling in CLI
- **Issue**: `main.rs` uses `.expect()` for JSON parsing; silently fails on stdin read
- **Work**:
  - Replace `.expect()` with `?` propagation
  - Add context to `anyhow::Error` messages (e.g., "Input JSON parse failed at line X")
  - Return non-zero exit code on errors
- **Acceptance**: Errors are logged with actionable context

### 2.2 Patch Parsing Robustness
- **Issue**: `diffy::Patch::from_str()` may not handle all unified diff formats
- **Work**:
  - Test against non-standard diff outputs (git, unified, context formats)
  - Add fallback diff parser if needed
  - Document supported diff format in README
- **Assumption**: Assuming unified diff format; needs verification with real APR systems

### 2.3 Validate Configuration
- **Issue**: Invalid configs (e.g., negative weights, empty commands) silently pass
- **Work**:
  - Add validation method to each config struct
  - Return `Diagnostic::Error` if `timeout_secs == 0` or weights are invalid
  - Document constraints in model docs
- **Acceptance**: Invalid configs produce clear error diagnostics

---

## Phase 3: Features & Completeness

### 3.1 Add More Semantic Checkers
- **Priority Order**:
  1. **LinterChecker**: Run style/lint tools (rustfmt, clippy, pylint, etc.)
  2. **StaticAnalysisChecker**: Integration with SAT/SMT solvers or existing tools
  3. **CustomCheckChecker**: Allow user-provided shell scripts as checkers
- **Pattern**: Each checker implements `SemanticChecker` trait with early exit on fatal errors

### 3.2 Expand Diagnostic Information
- **Work**:
  - Add `SourceLocation` population for compile/test errors
  - Parse line:column from compiler output
  - Link diagnostics to specific diff hunks in mode 1–2–4 reasoning
- **Acceptance**: Diagnostics pinpoint exact file/line of failures

### 3.3 Language-Specific Support
- **Work**:
  - Parameterize `compile_command` / `test_command` by language (Rust, Bash, Markdown)
  - Provide defaults in CLI or config template
  - Document per-language setup requirements
- **Acceptance**: Out-of-box support for 3 languages without config tweaking

### 3.4 Similarity Metric Improvements
- **Current**: `compute_distance()` sums weights; doesn't penalize structural differences
- **Work**:
  - Consider tree-based diff (AST) for semantic similarity
  - Add hunk locality penalty (scattered changes worse than contiguous)
  - Profile performance impact
- **Priority**: Lower; impacts Mode 1–2–4 ranking but not correctness

---

## Phase 4: Testing & Documentation

### 4.1 Unit Tests
- **Scope**:
  - `normalize_text()` with edge cases (CRLF, trailing spaces, empty lines)
  - `compute_distance()` with various patch types
  - Mode resolution logic (`resolve_mode()`)
  - JSON serialization round-trip for all model types
- **Coverage Target**: ≥80% for `core.rs` and `models.rs`

### 4.2 Integration Tests
- **Scope**:
  - End-to-end Mode 1–4 evaluation with mock patches
  - Subprocess invocation with timeout (mock subprocess if needed)
  - CLI arg parsing and JSON I/O
  - Error recovery (malformed input, missing fields)
- **Test Data**: Include sample JSON requests + expected responses per mode

### 4.3 Documentation
- **README**: Quick-start, modes explained, CLI usage, config schema
- **docs/ARCHITECTURE.md**: Design rationale, mode decision tree, extensibility (adding checkers)
- **docs/INTEGRATION.md**: Subprocess expectations, diff format, example client usage
- **In-code**: Doc comments on public APIs, assumptions in complex logic

### 4.4 Performance Benchmarks
- **Baseline**: Evaluate 100 candidates against 10 perfect patches → target <1s
- **Profile**: Identify bottlenecks in patch diffing and text normalization
- **Acceptance**: Document performance budget and scaling limits

---

## Phase 5: Production Hardening

### 5.1 Logging & Observability
- **Work**:
  - Add `tracing` or `log` crate for debug/info/warn/error levels
  - Log mode resolution, checker execution, decision rationale
  - Structured logging for JSON output compatibility
- **Acceptance**: Debugging without re-running difficult scenarios

### 5.2 Dependency Review
- **Issue**: `anyhow` is used but errors are JSON-serialized; may not round-trip
- **Work**:
  - Audit error types for JSON compatibility
  - Add custom error struct if needed
  - Pin versions or document MSRV (Minimum Supported Rust Version)

### 5.3 Subprocess Safety
- **Work**:
  - Validate `compile_command` / `test_command` for shell injection risks
  - Run subprocess with restricted environment (no secrets in subprocess env)
  - Implement memory/CPU limits where OS allows (cgroups, etc.)
- **Assumption**: Commands are trusted from config source; user input not directly used

### 5.4 Concurrency (Future)
- **Note**: Current implementation is single-threaded
- **Work** (if needed):
  - Parallelize candidate evaluation in Mode 3
  - Isolate subprocess state (no global mutable state)
  - Document thread-safety guarantees of checkers

---

## Known Unknowns

- **Diff Format**: Assumed unified diff; verify against real APR system outputs
- **Subprocess Defaults**: No language-specific defaults exist; must be configured externally
- **Patch Size Limits**: No bounds on candidate/perfect patch count; test at scale
- **Whitespace Normalization**: `ignore_whitespace` trims all lines; may over-normalize (e.g., indentation-sensitive languages)

---

## Milestones

| Phase | Target | Blocker Clearance |
|-------|--------|-------------------|
| Phase 1 | 2 weeks | Mode 3 functional |
| Phase 2 | 1 week | Production-safe error handling |
| Phase 3 | 4 weeks | Feature-complete for 3+ languages |
| Phase 4 | 2 weeks | ≥80% test coverage |
| Phase 5 | 2 weeks | Ready for beta deployment |

**Total Estimate**: 11 weeks (with parallelization potential in phases 3–5)

---

## Non-Goals (For Now)

- GPU acceleration or large-scale parallelization (revisit at >10k candidates/run)
- DSL for patch synthesis (out of scope; refiner only ranks/selects)
- Visual UI or web interface (CLI + JSON is sufficient)
- Support for patch formats other than unified diff
