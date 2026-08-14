# patch-refinement

A Rust library and CLI tool for **AI patch refinement** in automated program
repair (APR) and self-healing code frameworks.

It validates AI-generated patch candidates syntactically (parse and apply),
producing **structured, machine-readable decisions** and diagnostics.

## What it does

`patch-refinement`:

- Accepts one or more source files (keyed by path) and one or more
  AI-generated patch candidates, each naming which file it targets.
- Optionally compares them against **reference ("perfect") patches** with
  structured reasoning (SingleExemplar/MultiExemplar).
- Validates patches syntactically: does the patch parse (unified diff format)
  and apply cleanly to its target file?
- Returns a **structured JSON response** with:
  - `decision`: `approved`, `rejected`, or `failed`
  - Selected patch ID (if any)
  - Matched perfect patch ID (if applicable, SingleExemplar/MultiExemplar)
  - Structured deviations (how the AI patch differs from the closest perfect
    patch, SingleExemplar/MultiExemplar)
  - Structured reasoning (when available, SingleExemplar/MultiExemplar)
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

### Example: SingleExemplar (Rust, with a perfect patch and reasoning)

```json { "files": { "main.rs": "fn main() {\n  println!(\"Hello\");\n}" },
"candidates": [ { "id": "ai_patch_1", "target_path": "main.rs",
"diff_content": "--- a/main.rs\n+++ b/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-
println!(\"Hello\");\n+  println!(\"Hello, world!\");\n }" } ],
"perfect_patches": [ { "id": "perfect_1", "target_path": "main.rs",
"diff_content": "--- a/main.rs\n+++ b/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-
println!(\"Hello\");\n+  println!(\"Hello, world!\");\n }", "reason": {
"summary": "Improve greeting message.", "details": [ { "hunk_index": 0,
"start_line": 2, "end_line": 2, "text": "Updated greeting for clarity." } ] } }
], "problem_statement": "Improve greeting message.", "config": { "language":
"rust", "file_path": "main.rs", "whitespace": { "ignore_whitespace": true,
"normalize_line_endings": true } } } ```

### Example: SyntacticOnly (syntactic validation only)

```json { "files": { "main.rs": "fn main() {\n  println!(\"Hello\");\n}" },
"candidates": [ { "id": "ai_patch_1", "target_path": "main.rs",
"diff_content": "--- a/main.rs\n+++ b/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-
println!(\"Hello\");\n+  println!(\"Hello, world!\");\n }" } ],
"perfect_patches": null, "problem_statement": "Improve greeting message.",
"config": { "language": "rust", "file_path": "main.rs" } } ```

### Example: multiple files in one request

`files` may hold more than one entry; each candidate/perfect patch declares
which one it targets via `target_path` (always required — never inferred from
the diff's own `--- a/`/`+++ b/` headers). A candidate is only ever compared
against exemplars sharing the same `target_path`:

```json { "files": { "a.rs": "fn a() {}\n", "b.rs": "fn b() {}\n" },
"candidates": [ { "id": "ai_patch_1", "target_path": "b.rs", "diff_content":
"--- a/b.rs\n+++ b/b.rs\n@@ -1 +1 @@\n-fn b() {}\n+fn b() { println!(); }\n" }
], "perfect_patches": null } ```

When `perfect_patches` is `null` or empty, SyntacticOnly is inferred: patch_refiner
validates that the patch parses and applies cleanly. The caller (e.g., `ruchat`)
is responsible for any semantic validation (compile checks, tests, linting).

## Modes explained

The module supports three application modes. The mode can be:

- **Inferred** from the presence/absence of `perfect_patches`, or
- **Explicitly set** via `config.mode_override`.

### SingleExemplar – One known perfect patch

**Inputs:**

- `files` (with an entry for each targeted path)
- One or more AI-generated patch candidates
- Exactly one **perfect patch**, with or without structured reasoning
- Optional `problem_statement`

**Behavior:**

- If any AI-generated patch **exactly matches** the perfect patch (modulo
  configured whitespace/normalization rules):
  - `decision`: `approved`
  - `selected_patch_id`: ID of the matching AI patch
  - `matched_perfect_patch_id`: ID of the matched perfect patch
  - `reasoning`: reused structured reasoning from the perfect patch, if it has
    any (otherwise omitted — no reasoning is inferred or fabricated)
- If no AI-generated patch matches:
  - `decision`: `rejected`
  - `deviations`:
    - `closest_perfect_patch_id`
    - `diff_from_perfect`: unified diff between AI-patched code and
      perfect-patched code
    - `distance_score`: numeric similarity score (lower is better)
  - `reasoning`: reasoning from the closest perfect patch, if it has any

Use SingleExemplar when you have a **golden patch** and want the AI to
reproduce it, with explanations included whenever the exemplar carries them.

---

### SyntacticOnly – Syntactic validation only

**Inputs:**

- `files` (with an entry for each targeted path)
- AI-generated patch candidates
- Optional `problem_statement`
- **No** reference patches

**Behavior:**

- Perform **syntactic validation only**:
  - Does the patch parse as a valid unified diff?
  - Does the patch apply cleanly to its `target_path` entry in `files` without hunk mismatches?
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

Use SyntacticOnly when you do **not** have golden patches and rely on
syntactic validation as a precondition for the caller's semantic checks.

---

### MultiExemplar – Multiple competing perfect patches

**Inputs:**

- `files` (with an entry for each targeted path)
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

Use MultiExemplar when multiple correct implementations exist and you want to
know which pattern the AI approximated.

## CLI usage

```bash patch-refiner [OPTIONS]

OPTIONS: -i, --input <FILE>   Path to JSON request file (reads from stdin if
omitted) --mode <MODE>    Explicit mode override (syntactic_only,
single_exemplar, multi_exemplar) --ignore-whitespace Ignore whitespace when
comparing patches -h, --help Print help -V, --version        Print version ```

Examples:

```bash
# From file
patch-refiner --input request.json > response.json

# From stdin with explicit mode
cat request.json | patch-refiner --mode syntactic_only > response.json ```

## Error handling

All errors are reported in the `diagnostics` array of the `RefinementResponse`.
Each diagnostic is a structured object:

```json { "level": "error", "category": "patch_apply", "message": "Patch failed
to apply: hunk 1 does not match", "location": { "file": "main.rs", "line": 5,
"column": null } } ```

Common diagnostic categories (applicable to patch_refiner's syntactic scope):

- `patch_parse`: Invalid diff format.
- `patch_apply`: Patch could not be applied to the original code.
- `similarity`: Issues or warnings related to similarity computation (exemplar
  modes only).
- `other`: Miscellaneous errors.

The top-level `decision` will be:

- `approved`: At least one candidate passed all syntactic checks (and, in
  exemplar modes, matched a perfect patch if required).
- `rejected`: No candidate matched a perfect patch (SingleExemplar/MultiExemplar), but patches
  were syntactically valid.
- `failed`: No candidate passed syntactic validation (e.g., all failed to parse
  or apply in SyntacticOnly).

Callers should inspect both `decision` and `diagnostics` to determine next
steps. In SyntacticOnly, an `approved` response indicates the patch is a safe syntactic
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
