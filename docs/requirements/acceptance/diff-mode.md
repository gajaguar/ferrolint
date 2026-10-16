---
type: rule
title: Acceptance criteria - diff mode
description: Given/When/Then criteria for --diff-base changed-file selection, mapped to FR-08.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: diff mode

## AC-22: --diff-base lints only changed files (FR-08)

* Given a tree whose working state changed some files, added one and
  left the rest untouched since merge base of `HEAD` and `--diff-base
  HEAD~1`,
* when a run starts with that argument,
* then only the changed and added files are parsed and plugin-walked,
  find only their findings report, and the run summary names the diff
  mode and the selected file count.

## AC-23: An unresolvable diff base is a usage error (FR-08)

* Given a run started with `--diff-base` naming a ref that does not
  resolve in the repository,
* when the argument is processed,
* then no file is linted, the resolution failure is on stderr, and the
  process exits with 2 - never with a partial or full run against a
  guessed base.

A finding inside a diff-selected file is filtered by the same
[configuration](../configuration.md), suppressions included, as in
a full run; nothing in diff mode changes a finding's verdict.
