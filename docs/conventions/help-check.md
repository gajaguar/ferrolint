---
type: rule
title: Help-line check
description: make help-check fails when a Makefile target has no ## help line, so make help lists every target.
tags: [makefile]
status: stable
---

# Help-line check

`make help` prints the `##` comment of each target, so a target without one
is invisible to anyone who discovers the command surface through it.
`make help-check` scans `Makefile` and `mk/*.mk` for lines that define a
target and fails, naming the file and line, when one has no `##`. It runs as
part of `make check`.

A target whose name starts with `_` is private and exempt: it is an
implementation detail of a public target, such as `_md-fix-scoped`.
Variable assignments (`:=`, `?=`) and `.PHONY` or other dot-directives are
not targets and are skipped.

The check reads the target line only. A line that declares several targets
at once, or a target defined inside a `define` block, is out of its reach.
