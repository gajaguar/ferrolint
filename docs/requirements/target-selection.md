---
type: rule
title: Target selection
description: Which source files a run covers - explicit targets, gitignore honouring and configured excludes.
tags: [requirements, cli]
status: stable
---

# Target selection

`FR-01`: a run lints the files it is pointed at, and only those.

* Targets are one or more files or directories on the command line. A
  directory expands recursively to the Python files (`*.py`) inside it.
* Every file discovered inside a target that git ignores - by `.gitignore`
  files, the global ignore file or `.git/info/exclude` - is skipped, using
  the same semantics as `git check-ignore`.
* A configured `exclude` list in the active configuration adds patterns on
  top: a match removes the file from the run the same way gitignore does.
* Files named explicitly on the command line are linted even when an
  ignore rule covers them; the ignore/exclusion machinery applies to
  discovery inside targets, not to named arguments.
* Non-Python files do not reach the parser, discovered or named.

The run reports how many files it linted per scope, so an exclusion that
silently swallows a tree is visible in the output.
