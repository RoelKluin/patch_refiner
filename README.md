# patch-refinement

A Rust library and CLI tool for **AI patch refinement** in automated program
repair (APR) and self-healing code frameworks.

It validates AI-generated patch candidates syntactically (parse and apply),
producing **structured, machine-readable decisions** and diagnostics.

## What it does

`patch-refinement`:

- Accepts original source code and one or more AI-generated patch candidates.
- Optionally compares them against **reference ("perfect") patches** with
  structured reasoning (Modes 1, 2, 4).
- Validates patches syntactically: does the patch parse (unified diff format)
  and apply cleanly to `original_code`?
- Returns a **structured JSON response** with:
  - `decision`: `approved`, `rejected`, or `failed`
  - Selected patch ID (if any)
  - Matched perfect patch ID (if applicable, Modes 1/2/4)
  - Structured deviations (how the AI patch differs from the closest perfect
    patch, Modes 1/2/4)
  - Structured reasoning (when available, Modes 1/2/4)
  - Diagnostics (parse errors, apply errors, etc.)

**Semantic validation (compile checks, test execution, linting) is intentionally
out of scope** and delegated to the calling system (e.g., `ruchat`). This avoids
duplicating subprocess sandboxing and allows patch_refiner to remain a pure,
library-first component.

It is designed to be integrated into larger APR or self-healing pipelines as
either:

- A **library** (`patch_refiner` crate), or
- A **CLI binary** (`patch-refiner`) that reads JSON from stdin/file and writes
  JSON to stdout.

## Quickstart

1. **Build the project**:

   ```bash cargo build --release --features cli ```

2. **Prepare a JSON request** (see [Configuration](#configuration) and [Modes
explained](#modes-explained)):

   ```bash cat request.json ```

3. **Run the CLI**:

   ```bash cargo run --release --features cli -- --input request.json > response.json ```

   Or pipe via stdin:

   ```bash cat request.json | cargo run --release --features cli -- > response.json ```

4. **Inspect the structured response**:

   ```bash cat response.json ```

## Installation

```bash cargo build --release ```

The binary will be available at:

```bash ./target/release/patch-refiner ```

You can also use it as a library by adding to your `Cargo.toml`:

```toml [dependencies] patch_refiner = { path = "path/to/patch-refinement" } ```

## Configuration

Configuration is passed as part of the JSON request under the `config` field.
See `src/models.rs` for the authoritative Rust structs and resulting JSON
schema.

### Example: Mode 1 (Rust, with perfect patches and reasoning)

```json { "original_code": "fn main() {\n  println!(\"Hello\");\n}",
"candidates": [ { "id": "ai_patch_1", "diff_content": "--- a/main.rs\n+++
b/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-  println!(\"Hello\");\n+
println!(\"Hello, world!\");\n }" } ], "perfect_patches": [ { "id": "perfect_1",
"diff_content": "--- a/main.rs\n+++ b/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-
println!(\"Hello\");\n+  println!(\"Hello, world!\");\n }", "reason": {
"summary": "Improve greeting message.", "details": [ { "hunk_index": 0,
"start_line": 2, "end_line": 2, "text": "Updated greeting for clarity." } ] } }
], "problem_statement": "Improve greeting message.", "config": { "language":
"rust", "file_path": "main.rs", "whitespace": { "ignore_whitespace": true,
"normalize_line_endings": true } } } ```

### Example: Mode 3 (syntactic validation only)

```json { "original_code": "fn main() {\n  println!(\"Hello\");\n}",
"candidates": [ { "id": "ai_patch_1", "diff_content": "--- a/main.rs\n+++
b/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-  println!(\"Hello\");\n+
println!(\"Hello, world!\");\n }" } ], "perfect_patches": null,
"problem_statement": "Improve greeting message.", "config": { "language":
"rust", "file_path": "main.rs" } } ```

When `perfect_patches` is `null` or empty, Mode 3 is inferred: patch_refiner
validates that the patch parses and applies cleanly. The caller (e.g., `ruchat`)
is responsible for any semantic validation (compile checks, tests, linting).

## Modes explained

The module supports four application modes. The mode can be:

- **Inferred** from the presence/absence of `perfect_patches` and their
  `reason`, or
- **Explicitly set** via `config.mode_override`.

### Mode 1 – Known perfect patch(es) and known reasoning

**Inputs:**

- `original_code`
- One or more AI-generated patch candidates
- One or more **perfect patches**, each with **structured reasoning**
- Optional `problem_statement` and high-level reason

**Behavior:**

- If any AI-generated patch **exactly matches** a perfect patch (modulo
  configured whitespace/normalization rules):
  - `decision`: `approved`
  - `selected_patch_id`: ID of the matching AI patch
  - `matched_perfect_patch_id`: ID of the matched perfect patch
  - `reasoning`: Reused structured reasoning from the perfect patch
- If no AI-generated patch matches:
  - `decision`: `rejected`
  - `deviations`:
    - `closest_perfect_patch_id`
    - `diff_from_perfect`: unified diff between AI-patched code and
      perfect-patched code
    - `distance_score`: numeric similarity score (lower is better)
  - `reasoning`: reasoning from the closest perfect patch, if available

Use Mode 1 when you have **golden patches** and want the AI to reproduce them
with explanations.

---

### Mode 2 – Known perfect patch(es), no reasoning

**Inputs:**

- `original_code`
- AI-generated patch candidates
- One or more perfect patches **without** detailed reasoning

**Behavior:**

- Same matching logic as Mode 1.
- On exact match:
  - `decision`: `approved`
  - No `reasoning` included (since none exists).
- On mismatch:
  - `decision`: `rejected`
  - `deviations` as in Mode 1.
  - No attempt to infer or fabricate reasoning.

Use Mode 2 when you have reference patches but no structured rationale.

---

### Mode 3 – Syntactic validation only

**Inputs:**

- `original_code`
- AI-generated patch candidates
- Optional `problem_statement`
- **No** reference patches

**Behavior:**

- Perform **syntactic validation only**:
  - Does the patch parse as a valid unified diff?
  - Does the patch apply cleanly to `original_code` without hunk mismatches?
- If a candidate passes syntactic checks:
  - `decision`: `approved`
  - `selected_patch_id`: ID of the passing candidate
  - `diagnostics`: any warnings/info from checks
- If no candidate passes:
  - `decision`: `failed`
  - `diagnostics`: structured errors explaining why each candidate failed (parse
    error, apply error, etc.)

**Note:** Semantic validation (compile checks, test execution, static analysis)
is **not performed by patch_refiner**. The caller (e.g., `ruchat`) is
responsible for running semantic checks on approved candidates using its own
sandboxed subprocess infrastructure.

Use Mode 3 when you do **not** have golden patches and rely on syntactic
validation as a precondition for the caller's semantic checks.

---

### Mode 4 – Multiple competing perfect patches

**Inputs:**

- `original_code`
- AI-generated patch candidates
- Multiple perfect patches representing **different valid solutions**, each with
  its own reasoning

**Behavior:**

- If an AI patch matches any perfect patch:
  - `decision`: `approved`
  - `matched_perfect_patch_id`: ID of the matched perfect patch
  - `reasoning`: reasoning from that perfect patch (indicates which solution
    pattern was used)
- If no exact match:
  - `decision`: `rejected`
  - `deviations`:
    - Closest perfect patch ID
    - Diff and distance score
  - `reasoning`: reasoning from the closest perfect patch, optionally annotated
    to highlight mismatches

Use Mode 4 when multiple correct implementations exist and you want to know
which pattern the AI approximated.

## CLI usage

```bash patch-refiner [OPTIONS]

OPTIONS: -i, --input <FILE>   Path to JSON request file (reads from stdin if
omitted) --mode <MODE>    Explicit mode override (mode1, mode2, mode3, mode4)
--ignore-whitespace Ignore whitespace when comparing patches -h, --help
Print help -V, --version        Print version ```

Examples:

```bash
# From file
patch-refiner --input request.json > response.json

# From stdin with explicit mode
cat request.json | patch-refiner --mode mode3 > response.json ```

## Error handling

All errors are reported in the `diagnostics` array of the `RefinementResponse`.
Each diagnostic is a structured object:

```json { "level": "error", "category": "patch_apply", "message": "Patch failed
to apply: hunk 1 does not match", "location": { "file": "main.rs", "line": 5,
"column": null } } ```

Common diagnostic categories (applicable to patch_refiner's syntactic scope):

- `patch_parse`: Invalid diff format.
- `patch_apply`: Patch could not be applied to the original code.
- `similarity`: Issues or warnings related to similarity computation (Modes
  1/2/4 only).
- `other`: Miscellaneous errors.

The top-level `decision` will be:

- `approved`: At least one candidate passed all syntactic checks (and, in Modes
  1/2/4, matched a perfect patch if required).
- `rejected`: No candidate matched a perfect patch (Modes 1/2/4), but patches
  were syntactically valid.
- `failed`: No candidate passed syntactic validation (e.g., all failed to parse
  or apply in Mode 3).

Callers should inspect both `decision` and `diagnostics` to determine next
steps. In Mode 3, an `approved` response indicates the patch is a safe syntactic
precondition for the caller's semantic checks (compile, test, lint); the
caller's own subprocess execution remains responsible for proving correctness.

## JSON schema reference

The authoritative schema is defined by the Rust types in `src/models.rs`:

- `RefinementRequest`
- `RefinementResponse`
- `PatchCandidate`
- `PerfectPatch`
- `Reason` / `ReasonDetail`
- `RefinementConfig`, `LanguageWeights`, `WhitespaceConfig`
- `Diagnostic`, `DiagnosticLevel`, `DiagnosticCategory`, `SourceLocation`

For most use cases, follow the examples in this README and adjust fields as
needed.

## License

MIT
