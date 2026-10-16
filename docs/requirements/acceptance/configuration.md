---
type: rule
title: Acceptance criteria - configuration
description: Given/When/Then criteria for target selection and configuration, mapped to FR-01 and FR-02.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: configuration

## AC-01: Configuration loads from pyproject.toml (FR-02)

* Given a repository with a `[tool.ferrolint]` section in
  `pyproject.toml`,
* when a run starts in that directory,
* then the run uses that section's settings and exits with a result code
  (0 or 1), not a configuration error.

## AC-02: ferrolint.toml wins over pyproject.toml (FR-02)

* Given a directory holding both a `ferrolint.toml` and a
  `[tool.ferrolint]` table in `pyproject.toml`, with deliberately
  different settings,
* when a run starts,
* then the effective configuration is `ferrolint.toml`'s and nothing
  from the `[tool.ferrolint]` table applies.

## AC-03: Plugin SHA mismatch is a configuration error (FR-02)

* Given a configuration whose plugin entry's `sha256` does not match the
  file at `path`,
* when a run starts,
* then no file is linted, the mismatch is reported on stderr, and the
  process exits with code 2.

## AC-04: select and ignore filter findings (FR-02)

* Given a configuration selecting a code and ignoring another code that
  both fire on the same file, plus a `per-file-ignores` entry that
  ignores the selected code for a named glob,
* when a run lints that file, once inside the glob and once outside,
* then the ignored code never appears, the selected code only reports
  outside the glob, and the same filtering holds in every output format.

## AC-05: A run honours gitignore and configured excludes (FR-01)

* Given a target directory containing Python files, some matched by
  `.gitignore` and some matched only by the configured `exclude` list,
* when a run lints the directory,
* then ignored files are absent from the report and from the file count,
  and the run is byte-identical to the same run with those files
  removed from the tree.

## AC-06: Explicitly named files bypass discovery filters (FR-01)

* Given a `.gitignore`-covered Python file named explicitly on the
  command line,
* when a run starts,
* then the file is linted and its findings report normally.
