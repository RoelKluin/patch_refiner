#!/usr/bin/env bash
# Runs each remaining line in tasks.txt through 'claude -p', gated by
# cargo fmt/clippy/test, committing successes to an isolated overnight
# branch. Never pushes. Completed tasks move to tasks_done.txt so a
# rerun resumes where it left off; failed tasks stay in tasks.txt for
# retry on the next run.
set -euo pipefail

# --- Self-modification hardening --------------------------------------------
# Tonight (2026-08-13) item 13's task edited *this file* while a previous
# invocation of this loop was still executing it, with two bad consequences.
# First, the edit had zero effect on the rest of that run: bash parses an
# entire compound command (the whole `while` loop below is one such command)
# before running any of it, and from then on it reads the *already-open file
# descriptor* by byte offset as execution proceeds rather than re-reading the
# file by path -- so the run kept using the old, pre-edit gate logic, and
# item 14 landed a change that broke the ranting_i18n sibling crate because
# the improved multi-crate gate the edit introduced never actually took
# effect. Second, because the edit grew the file by roughly 1.5KB *above* the
# loop body, bash's saved byte offset for "resume after the loop ends" then
# pointed into the middle of the loop body in the edited file on disk, so the
# end-of-run cleanup and summary code could be skipped, re-entered, or
# misparsed entirely.
#
# The fix: at startup, copy this script to a private mktemp path and exec
# that copy with the original arguments. Bash then parses and runs the copy,
# which no task running later in the same invocation can edit out from under
# it -- any task that edits scripts/overnight_loop.sh on disk changes the
# *next* run, never this one. RANTING_OVERNIGHT_REEXEC guards against the
# copy re-copying (and re-exec'ing) itself, which would otherwise recurse
# forever. Do not remove this block, and do not "simplify" it by dropping the
# guard or the re-exec.
if [[ -z "${RANTING_OVERNIGHT_REEXEC:-}" ]]; then
  export RANTING_OVERNIGHT_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  tmp_self="$(mktemp "${TMPDIR:-/tmp}/overnight_loop.XXXXXX.sh")"
  cp "${BASH_SOURCE[0]}" "$tmp_self"
  chmod +x "$tmp_self"
  export RANTING_OVERNIGHT_REEXEC=1
  exec "$tmp_self" "$@"
fi
# Reached only inside the re-exec'd copy (a `trap ... EXIT` set before `exec`
# would never fire -- exec replaces the process image, traps and all -- so
# the cleanup trap has to live here, after the guard, referring to the
# copy's own path via BASH_SOURCE[0]).
trap 'rm -f "${BASH_SOURCE[0]}"' EXIT

REPO_ROOT="${RANTING_OVERNIGHT_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$REPO_ROOT"

TASKS_FILE="${TASKS_FILE:-tasks.txt}"
DONE_FILE="${DONE_FILE:-tasks_done.txt}"
# Per-repo, not a shared $HOME/logs: every overnight loop on the machine wrote
# to the same log and the same failures file, so entries from other repos
# interleaved with this one's and neither file could be trusted to describe a
# given run. Kept outside the repo deliberately -- a log directory inside it
# would be swept up by the per-task 'git add -A' below, and would trip the
# working-tree-clean pre-flight check in any repo that had not gitignored it.
LOG_DIR="${LOG_DIR:-$HOME/logs/$(basename "$REPO_ROOT")}"
DATE_TAG="$(date +%F)"
LOG_FILE="$LOG_DIR/$DATE_TAG.log"
FAIL_FILE="$LOG_DIR/failures"
MAX_TASKS="${MAX_TASKS:-0}" # 0 = unlimited
DRY_RUN=0
ALLOWED_TOOLS="Read Edit Write Bash(cargo *) Bash(git *)"

# This repo is not a cargo workspace -- ranting, ranting_core, ranting_derive
# and ranting_i18n each have their own Cargo.toml/Cargo.lock -- so a gate that
# only ran at the repo root never compiled ranting_i18n at all. Discovered by
# listing every directory with its own Cargo.toml, so a future sibling crate
# is picked up automatically without editing this script again.
gate_dirs() {
  local f
  for f in "$REPO_ROOT"/Cargo.toml "$REPO_ROOT"/*/Cargo.toml; do
    [[ -f "$f" ]] && dirname "$f"
  done
}

# Runs fmt/clippy/test in one crate directory, logging which directory failed
# so a gate failure points straight at the offending crate instead of forcing
# a manual re-run per directory to find it.
run_gate_in() {
  local dir="$1"
  echo "--- gate: $dir ---" | tee -a "$LOG_FILE"
  (
    cd "$dir" &&
      cargo fmt --check &&
      cargo clippy -- -D warnings &&
      cargo test
  ) >>"$LOG_FILE" 2>&1
}

# Runs the gate in every sibling crate directory; fails (and reports which
# directory) on the first failure.
run_gate() {
  local dir
  while IFS= read -r dir; do
    if ! run_gate_in "$dir"; then
      echo "Gate failed in $dir" | tee -a "$LOG_FILE" >&2
      return 1
    fi
  done < <(gate_dirs)
  return 0
}

usage() {
  cat <<EOF
Usage: $(basename "$0") [--dry-run] [--max-tasks N]

  --dry-run        Print what would run without invoking claude or git.
  --max-tasks N    Only process the first N remaining tasks this run.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --max-tasks) MAX_TASKS="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
  esac
done

mkdir -p "$LOG_DIR"
touch "$DONE_FILE"

if [[ ! -s "$TASKS_FILE" ]]; then
  echo "No tasks remaining in $TASKS_FILE." | tee -a "$LOG_FILE"
  exit 0
fi

if [[ -n "$(git status --porcelain)" ]]; then
  echo "Working tree not clean; commit or stash before running." >&2
  exit 1
fi

BRANCH="overnight/$DATE_TAG"
if git rev-parse --verify --quiet "$BRANCH" >/dev/null; then
  echo "Branch $BRANCH already exists; refusing to reuse it. Delete it or rerun tomorrow." >&2
  exit 1
fi

if [[ "$DRY_RUN" -eq 0 ]]; then
  git checkout -b "$BRANCH"
else
  echo "[dry-run] would create branch $BRANCH"
fi

# Pre-flight: if the gate itself is already broken on the starting commit,
# every task below is unwinnable regardless of what it changes -- fail fast
# instead of burning the whole run stashing 12 doomed attempts.
if [[ "$DRY_RUN" -eq 0 ]]; then
  echo "=== Pre-flight gate check ===" | tee -a "$LOG_FILE"
  if ! run_gate; then
    echo "Pre-flight gate failed on the starting commit -- fix that first, then rerun." | tee -a "$LOG_FILE" >&2
    echo "See $LOG_FILE for details." >&2
    exit 1
  fi
fi

n_done=0
n_failed=0
task_count=0

# Tasks are consumed one at a time, re-reading the file each iteration, so new
# lines appended to it *while the run is in progress* get picked up. The file is
# never rewritten wholesale -- a completed task is deleted from it individually,
# the moment it lands -- so an append can never be clobbered by a stale in-memory
# copy. (The previous mapfile-up-front + rewrite-at-the-end design silently
# discarded anything appended mid-run.)
declare -A attempted=()
# NOTE: identity here is the line's *text*, not its position or a stable id.
# If a task line is rewritten mid-run (not just appended), the new text is a
# new key in this array, so it is treated as never-attempted and can be
# picked up again in the same run even if the old text already failed here.
# That happened tonight -- item 13 was rewritten mid-run into a more specific
# instruction and the rewrite got run -- and it was useful, but it is a side
# effect of "line text is the identity," not a designed retry mechanism.
# Don't rely on rewriting a task line to force a re-attempt.

# First non-blank line not already tried this run. Failed tasks stay in the file
# for the next run but are marked attempted, so one run never retries them --
# without that, a failing task would be picked forever.
next_task() {
  local line
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" ]] && continue
    [[ -n "${attempted["$line"]:-}" ]] && continue
    printf '%s' "$line"
    return 0
  done <"$TASKS_FILE"
  return 1
}

# Stages what a task is expected to have produced, deliberately rather than
# blindly. Tonight, a bare 'git add -A' swept an untracked 81KB scratch file
# (tmp_failed_lib.rs, a copy of src/lib.rs a task left behind) straight into
# a commit. Extension alone can't tell scratch from genuine -- that file
# *was* a .rs file -- so filenames matching common scratch/backup patterns
# are refused regardless of extension, on top of only allowing recognized
# source/doc extensions through at all; anything declined is reported to the
# log rather than silently dropped, so a stray file is visible instead of
# either entering history unnoticed or disappearing without a trace.
stage_task_changes() {
  # Tracked modifications/deletions are always safe to stage: a task can only
  # touch files that were already under version control.
  git add -u

  local f base
  while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    base="$(basename -- "$f")"
    case "$base" in
      tmp_*|*_tmp|*_tmp.*|scratch_*|*.orig|*.bak|*.rej|*.swp|*~|core|core.*)
        echo "Declined to stage (looks like scratch/backup output): $f" |
          tee -a "$LOG_FILE"
        continue
        ;;
    esac
    case "$f" in
      *.rs | *.toml | *.md | *.txt | *.lock | *.sh)
        git add -- "$f"
        ;;
      *)
        echo "Declined to stage (not a recognized source/doc type): $f" |
          tee -a "$LOG_FILE"
        ;;
    esac
  done < <(git status --porcelain | sed -n 's/^?? //p')
}

# Delete one exact line, preserving everything else -- including lines appended
# since the run started. awk compares the whole line as a literal string (no
# regex), and writes via a temp file in the same directory so a concurrent
# appender never sees a half-written file.
drop_task() {
  local tmp
  tmp="$(mktemp "$TASKS_FILE.XXXXXX")"
  TASK_TO_DROP="$1" awk 'BEGIN { t = ENVIRON["TASK_TO_DROP"] } $0 != t' \
    "$TASKS_FILE" >"$tmp"
  mv "$tmp" "$TASKS_FILE"
}

while :; do
  [[ -f "$TASKS_FILE" ]] || break

  if [[ "$MAX_TASKS" -gt 0 && "$task_count" -ge "$MAX_TASKS" ]]; then
    echo "Reached --max-tasks $MAX_TASKS; stopping with tasks still queued." |
      tee -a "$LOG_FILE"
    break
  fi

  task="$(next_task)" || break
  attempted["$task"]=1
  task_count=$((task_count + 1))

  echo "=== TASK: $task ===" | tee -a "$LOG_FILE"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "[dry-run] would run: claude -p \"$task\" --permission-mode acceptEdits --allowedTools \"$ALLOWED_TOOLS\""
    continue
  fi

  set +e
  claude -p "$task" \
    --permission-mode acceptEdits \
    --allowedTools "$ALLOWED_TOOLS" \
    >>"$LOG_FILE" 2>&1
  claude_exit=$?
  set -e
  if [[ "$claude_exit" -ne 0 ]]; then
    echo "claude -p exited $claude_exit" >>"$LOG_FILE"
  fi

  if run_gate; then
    # Dequeue *before* committing, so the queue update is part of this task's
    # own commit. If it were left uncommitted, a later task's gate failure
    # would 'git stash push -u' it away along with that task's work, and the
    # finished task would reappear in the queue (verified: it did).
    drop_task "$task"
    if [[ -n "$(git status --porcelain)" ]]; then
      stage_task_changes
      git commit -m "auto: ${task:0:72}" >>"$LOG_FILE" 2>&1
    else
      echo "No changes produced; nothing to commit." >>"$LOG_FILE"
    fi
    echo "$task" >>"$DONE_FILE"
    n_done=$((n_done + 1))
    echo "DONE: $task" | tee -a "$LOG_FILE"
  else
    git stash push -u -m "failed: ${task:0:72}" >>"$LOG_FILE" 2>&1
    echo "FAILED: $task" >>"$FAIL_FILE"
    n_failed=$((n_failed + 1))
    echo "FAILED: $task" | tee -a "$LOG_FILE"
  fi
done

# Only remove the file once nothing is left in it -- a failed task, or one
# appended mid-run and not reached, must survive for the next run.
if [[ "$DRY_RUN" -eq 0 && -f "$TASKS_FILE" ]]; then
  if ! grep -q '[^[:space:]]' "$TASKS_FILE"; then
    rm -f "$TASKS_FILE"
  fi
fi

echo "=== Summary: $n_done done, $n_failed failed, branch $BRANCH ===" | tee -a "$LOG_FILE"
if [[ "$n_failed" -gt 0 ]]; then
  echo "See $FAIL_FILE and 'git stash list' for failed attempts." | tee -a "$LOG_FILE"
fi
echo "Nothing was pushed. Review with: git log master..$BRANCH" | tee -a "$LOG_FILE"
