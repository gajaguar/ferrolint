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
* Unknown keys are configuration errors, so a typo never silently disables
  a check.
* An invalid value, an unreadable file, or a missing path in `plugins`
  fails before the run parses any file, with exit code 2.

A valid configuration always produces the same effective ruleset, so two
runs in the same directory with the same command line behave identically.
