# Diff Format Specification

This document specifies the expected **patch (diff) format** for `patch-refinement`.

## Overview

`patch-refinement` uses the [`diffy`](https://docs.rs/diffy/) crate to parse, apply, and compare patches. All patch inputs must conform to a format that `diffy` can understand.

## Expected format

**Unified diff format** is required.

Supported sources include:

- `git diff` output (e.g., `git diff HEAD~1`)
- `diff -u` output (e.g., `diff -u original.rs modified.rs`)
- Any other tool that produces standard unified diffs compatible with `diffy`

### Example: Git-style unified diff

```diff
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,5 @@
 fn main() {
-    println!("Hello");
+    println!("Hello, world!");
     let x = 1;
 }
```

### Example: `diff -u` output

```diff
--- original.rs	2026-08-06 10:00:00.000000000 +0200
+++ modified.rs	2026-08-06 10:00:01.000000000 +0200
@@ -1,5 +1,5 @@
 fn main() {
-    println!("Hello");
+    println!("Hello, world!");
     let x = 1;
 }
```

Both examples are valid as long as they conform to unified diff syntax.

## How patches are used

`patch-refinement` uses patches in three ways:

1. **Parsing** – Each patch string is parsed via `diffy::Patch::from_str`.
2. **Application** – Patches are applied to the original code using `diffy::apply`.
3. **Comparison** – To compute deviations, the tool generates new diffs between:
   - AI-patched code
   - Perfect-patched code

All of these operations rely on `diffy`’s understanding of unified diffs.

## Known limitations

The following are **not supported** or are only partially supported:

- **Binary diffs**  
  `diffy` works on text only. Binary patches (e.g., Git binary diffs) will fail to parse.

- **Custom or non-standard hunk headers**  
  Non-standard extensions to unified diff format may cause parsing failures.

- **Renamed/moved files**  
  While `git diff` may include rename metadata, `diffy` focuses on line-based changes. Rename information is not used by `patch-refinement`.

- **Multiple files in a single patch (future work)**  
  Currently, the data model assumes a **single logical patch per candidate/perfect entry**.  
  If you pass a multi-file diff, behavior is implementation-defined and may not match expectations.  
  For now, prefer one patch per file, or ensure your tooling splits multi-file diffs before passing them in.

- **Context-only patches**  
  Patches that contain no additions or deletions (only context lines) are effectively no-ops and may be treated as invalid or ignored, depending on configuration.

## Recommendations

To ensure reliable behavior:

- Generate diffs with:
  - `git diff --no-binary` (to avoid binary blobs)
  - Standard unified diff options (e.g., `diff -u`)
- Avoid:
  - Custom diff formats
  - Binary files
  - Complex rename/move metadata as the sole content of a patch
- Test your diffs with `diffy` directly if you are unsure:

  ```rust
  use diffy::Patch;

  fn main() {
      let diff = std::fs::read_to_string("example.diff").unwrap();
      match Patch::from_str(&diff) {
          Ok(_) => println!("Valid unified diff"),
          Err(e) => eprintln!("Invalid diff: {}", e),
      }
  }
  ```

## Reference

- `diffy` crate documentation: https://docs.rs/diffy/
- `diffy` source and examples: https://github.com/bmwill/diffy

If you encounter a diff that you believe should be supported but is rejected, please file an issue with:

- The original code snippet
- The diff content
- The error message from `diffy` or `patch-refinement`
