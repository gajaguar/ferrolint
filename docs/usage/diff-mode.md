---
type: reference
title: Diff mode
description: Which files --diff-base REF lints, how it asks git, and how its report reads.
tags: [cli, git]
status: stable
---

# Diff mode

`--diff-base REF` lints only what changed: the files added, copied,
modified or renamed between the merge base of `REF` and `HEAD` and the
working tree, plus untracked files git does not ignore.

* `REF` is anything git resolves to a commit: `HEAD~1`, `origin/main`, a
  tag. One that does not resolve, or a run outside a git repository, is a
  usage error (exit 2) before any file is linted.
* The changed files still go through target selection: they must sit
  under the targets on the command line, and `exclude` and `.gitignore`
  drop them as in a full run.
* The summary names the mode: `3 files linted (diff mode against
  HEAD~1), 5 findings.` in text, `"mode": "diff"` and `"diff-base"` in the
  JSON summary.
* Findings, suppressions and exit codes work as in a full run over the
  selected files.
