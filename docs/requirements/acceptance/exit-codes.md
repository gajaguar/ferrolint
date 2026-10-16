---
type: rule
title: Acceptance criteria - exit codes
description: Given/When/Then criteria for the 0 / 1 / 2 exit codes, mapped to FR-07.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: exit codes

## AC-19: A clean run exits 0 (FR-07)

* Given a run over files with no findings under the active
  configuration,
* when it completes,
* then the process exits with 0 and the report (any format) carries the
  file count and zero findings.

## AC-20: A run with findings exits 1 (FR-07)

* Given a run over files with one or more findings that no
  [suppression](../inline-suppressions.md) or per-file ignore removes,
* when it completes,
* then the process exits with 1, the findings are on stdout as the
  chosen format, and the count of findings matches the report.

## AC-21: A broken run exits 2 (FR-07)

* Given any run whose startup cannot complete - an unresolvable CLI
  flag value, a malformed configuration file, a plugin path that does
  not exist, a plugin SHA mismatch, or a plugin that fails to
  instantiate against the contract,
* when it fails,
* then the process exits with 2, the cause is on stderr, stdout is
  empty of the chosen format's report, and the code is 2 regardless of
  whether findings-free files were runnable before the failure.
