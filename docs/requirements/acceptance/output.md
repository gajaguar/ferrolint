---
type: rule
title: Acceptance criteria - output
description: Given/When/Then criteria for the four output formats and pylint code aliases, mapped to FR-06 and FR-10.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: output

## AC-14: text output is the default and per-diagnostic (FR-06)

* Given a run with findings and no `--format` argument,
* when the report prints,
* then each diagnostic prints one line with path, line and column,
  severity, code and message, followed by a summary line with file and
  finding counts, and the same run with `--format text` prints the same
  bytes.

## AC-15: json output is machine-parseable (FR-06)

* Given the same run with `--format json`,
* when the report is produced,
* then it parses as canonical JSON and carries one object per diagnostic
  with code, message, range, severity and source path, plus a
  document-level summary, with no extra top-level surprises.

## AC-16: sarif output is SARIF 2.1.0 shaped (FR-06)

* Given the same run with `--format sarif`,
* when the report is produced,
* then a SARIF 2.1.0 reader parses it, every diagnostic appears as a
  result with its rule reference and location, and a SARIF consumer sees
  one severity it maps from the diagnostic's own.

## AC-17: junit output reads as a test report (FR-06)

* Given the same run with `--format junit`,
* when the report is produced,
* then it parses as JUnit XML, one testcase per linted file with one
  failure element per diagnostic of that file, and a CI system reading
  it sees failures matching the other formats one for one.

## AC-18: Pylint aliases resolve through the run (FR-10)

* Given a configuration whose `select` cites a ported checker by Pylint
  msg-id and symbol rather than by plugin code,
* when the run lints a fixture firing that checker and reports in every
  format,
* then the checker's findings appear with its resolved plugin code plus
  both aliases attached, and `ignore` accepting the same spellings
  removes them equally.

The exit codes of the common fixture run are checked in
[exit codes](exit-codes.md); here the four formats are compared
one-to-one on the same fixture run, and any format that changes
results rather than packaging is a failure.
