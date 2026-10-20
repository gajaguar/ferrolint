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
* The per-file rules run on the changed files only. The project rules
  (`E0401`, `E0402`, `E0611`, `R0401`, `R0801`) run over the whole tree
  the targets select, as in a full run, so a cycle or a duplicate that
  reaches an unchanged file is found. Their findings are then kept for the
  changed files: an import finding stays if the file it sits on changed; a
  cycle or a duplicate stays if any file of it changed, and is reported
  where a full run reports it, on the file checked last, which may be an
  unchanged one. A cycle or a duplicate among unchanged files only is
  dropped.
* The project phase keeps its single cache entry, keyed on the whole tree
  (see [result cache](result-cache.md)), so a diff run and a full run share
  it and an unchanged tree does not run the phase again. A cold run parses
  every file once for the phase, which a diff run without these rules
  enabled does not.
