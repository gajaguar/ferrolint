---
type: rule
title: Diff mode
description: Linting only what changed relative to a git ref with --diff-base.
tags: [requirements, cli]
status: stable
---

# Diff mode

`FR-08`: a run can scope itself to a change instead of a whole tree.

* `--diff-base REF` resolves `REF` as a commit or a resolvable name
  (`HEAD~1`, a branch, a tag), computes the merge base of `REF` and the
  working tree state, and lints only the files whose path differs between
  that merge base and the worktree: new, modified or renamed.
* The selected files still pass through [target
  selection](target-selection.md): an ignored path the change touches is
  dropped the same way as in a full run.
* Suppressions, configuration and per-file filters apply to diff-selected
  files exactly as to the whole tree, so the exit code reads the same way
  either way: 0 means no findings in the changed set.
* An unresolvable ref is a usage error (exit code 2) reported on stderr
  before any file is linted, because a half-known base would silently
  skip linting.
* With `--diff-base` given, unchanged files contribute nothing to the
  findings count, but the run summary still names the diff mode so a
  clean run cannot be mistaken for a full-tree run.
