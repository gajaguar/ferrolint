---
type: rule
title: Configuration
description: Where the linter reads its settings - the [tool.ferrolint] table or a ferrolint.toml file - and what the settings are.
tags: [requirements, config]
status: stable
---

# Configuration

`FR-02`: configuration lives in the project, in one of two places, and a run
fails fast when the result does not parse.

* The first source is the `[tool.ferrolint]` table of `pyproject.toml`. The
  second is a `ferrolint.toml` file in the working directory.
* When both are present, `ferrolint.toml` wins entirely: it does not merge
  with the `[tool.ferrolint]` table.
* Configuration sections:
  * `plugins`: one entry per plugin, each with a `path` to the WASM file
    and a `sha256` that must match the file byte for byte. A mismatch is a
    configuration error.
  * `select` and `ignore`: rule code lists that decide which codes run and
    which findings report. `ignore` beats `select` for a given code.
  * `per-file-ignores`: a pattern (an iterable of codes) - map from file
    glob pattern to a list of codes that never report for matching files.
  * `py-version`: the Python version the code targets, as the text
    `"MAJOR.MINOR"` (for example `"3.14"`), as in Pylint. From 3.14 on,
    annotations are evaluated late (PEP 649), so a name in an annotation may
    be defined later in the scope; a module with
    `from __future__ import annotations` gets the same treatment without the
    key. Without the key no version is assumed. Any other spelling is a
    configuration error.
  * `options`: a map of text values that every plugin receives with each
    file (see [Plugin contract](plugin-contract.md)); the other options of
    a Pylint file fill it when no ferrolint configuration exists.
* Unknown keys are configuration errors, so a typo never silently disables
  a check.
* An invalid value, an unreadable file, or a missing path in `plugins`
  fails before the run parses any file, with exit code 2.

A valid configuration always produces the same effective ruleset, so two
runs in the same directory with the same command line behave identically.
