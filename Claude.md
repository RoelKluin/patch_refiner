# Engineering Plugin Customization - ruchat

This file gives the `engineering` plugin (standup, review, debug, architecture,
incident, deploy-checklist, and their underlying skills) the repo-specific
facts it needs instead of generic defaults. Claude reads this automatically
when working in this repository.

## Project

**ruchat** - single-maintainer, local-first Rust CLI for AI chat and
multi-agent orchestration, built on **Ollama** (LLM inference) and
**ChromaDB** (vector store / RAG). No cloud dependency by default;
**Anthropic (Claude) is an opt-in chat-provider** (`--chat-provider
anthropic`), chat only — Anthropic has no embeddings API, so RAG/`memorize`/
recall stay Ollama-only unconditionally.

- Maintainer: Roelof J.C. Kluin (`roel.kluin@gmail.com`).
- License: MIT. Version: see `Cargo.toml` (pre-1.0 - breaking changes expected).
- Language: Rust (edition 2024). Manual release build: `cargo build --release`.

## Documentation Pointers

Read on demand, not preemptively:

- @ORCHESTRATION.md - stage machine, role roster, Context/Turn data model. Read before touching `Stage`/`Role`/orchestrator control flow.
- @CONTEXT.md - the shared `Context`/`Turn` append-only log. Read before any state-shape change.
- @ROADMAP.md - phased plan, milestone-gate methodology, positioning vs. LangGraph/CrewAI/AutoGen.
- @TODO.md - live, prioritized task list. Pull status from here, never from memory — it changes daily.
- @DONE.md - completed-task log. Log new completed work here as **one-liner: commit + critical context**, not a full commit message.
- `agent_debug/*.json` - fixture-driven stage-machine tests (see Testing Strategy).
- @README.md / @INSTALL.md - user-facing quickstart, install steps.

## Commands

```bash
cargo test --lib                    # unit tests only, no tests/ integration suite by design
cargo test --lib -- --ignored agent_eval   # live-model agentic evals (needs Ollama running, non-deterministic)
cargo clippy --lib --tests          # lint; new warnings block, pre-existing baseline tolerated
cargo fmt --check                   # formatting; must be clean, no pre-existing exceptions
cargo build --release               # release build
```

## Code Review Focus (repo-specific, non-obvious)

- **Error handling**: `thiserror` + `#[from]` / `anyhow` context, not
  `eprintln!`/`println!`/`unwrap`. Flag new `println!`/`eprintln!` in library
  code (`src/core`, `src/providers`) - `tracing` is standard here.
- **Tool safety invariants** (`src/core/agent/tools.rs`,
  `src/core/orchestrator/fs.rs`): `read_file`/`list_dir` must keep refusing
  paths that canonicalize outside the repo root; `apply_patch` must keep
  requiring the target be tracked by `git ls-files`, stay under
  `MAX_PATCH_DIFF_BYTES`, and match the plan's `FILES:` scope when declared.
  Any change loosening these is security-relevant.
- **No new generic shell/exec tool** — only specific, typed, schema-validated
  tools for Worker/Scoper. Treat a proposed shell/exec tool as a design
  regression, not a neutral addition.
- **Test placement**: tests live in `#[cfg(test)] mod tests` next to the code
  they test (types are `pub(crate)` — no black-box `tests/` suite).
- **`agent_debug/*.json` fixtures**: a PR touching `Role`, `Stage`, or
  `ToolName` must verify fixture JSON still matches (role names like
  `Critic_0`, not `Critic0` — a naming mismatch has shipped as a real bug
  before).

## Delegation Policy

- Boilerplate, trait impls, test scaffolding, first-pass review -> rust-local-* subagents.
- Long build/test/clippy output -> route through build-log-summarizer; never paste raw.
- Codebase context -> query the chromadb MCP tool before re-reading whole files.
  Collections: `repo_docs-*` (design docs), `repo_lessons-*` (per-run
  agent-decision reviews), `repo_src-*` (ctags chunks), `repo_hist-*` (commit
  history). `scripts/index_rag.sh` refreshes; runs automatically from
  `.git/hooks/post-commit`.
- Code you can already name -> ripgrep + targeted read beats RAG (repo is small).
- `repo_lessons-*` -> for "has this failure mode happened before?"-type questions grep can't answer.
- Reserve your own reasoning for: borrow-checker/lifetime issues, architecture,
  concurrency bugs, anything in the agent-loop core.
- All local-model delegation goes to `ollama-heavy` (:11434), one at a time —
  it shares the port with ruchat itself, so disable delegation while measuring
  a live run. Never dispatch to `ollama-light` (:11431, CPU-bound, no CUDA on
  Maxwell). See `references/gpu-and-ollama.md` in the ruchat-dev skill for detail.

## Testing Strategy

- `cargo test --lib` is the whole deterministic suite; stage-machine coverage
  uses `FakeLlmClient`/`FakeVectorStore` driven by `agent_debug/*.json`.
- Agentic evals (`core/agent/evals.rs`) are `#[ignore]`d, hit a live Ollama
  server, and are expected to be flaky by design — a red run signals
  prompt/model reliability, not necessarily a code bug.

## Deploy Checklist

No deployment target (CLI binary). "Deploy" = release build:

1. `cargo check` && `cargo test --lib` pass.
2. `cargo clippy --lib --tests` reviewed — no *new* warnings.
3. `cargo build --release`; confirm the `ruchat` symlink resolves.
4. Manual smoke test against a running Ollama server (`ruchat pipe "..."`),
   and against ChromaDB (`start_chroma.sh`) if the change touches RAG.

## Incident Response / Debug

"Incidents" = broken build, test regression, or stage-machine stall — no
production service.

- Reproduce deterministically with `--debug-sequence <file.json>` against an
  `agent_debug/*.json` fixture instead of a live Ollama/Chroma run.
- Traces live only in memory during a run; on finish an LLM-generated summary
  (goal, outcome, round-by-round verdict, lessons) is archived directly to
  `ruchat_traces/successes/` or `ruchat_traces/failures/` — no raw trace is
  ever written to disk.
- Pattern search across past runs:
  `grep -h '^LESSON:' ruchat_traces/{successes,failures}/*.md | sort | uniq -c | sort -rn`

## Standup / Activity

No project tracker or chat connector. Use `git log --oneline -15` / `git log --stat -5`.
