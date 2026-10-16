---
type: rule
title: Output formats
description: The four report formats a run can emit and the exit codes that follow every run.
tags: [requirements, output]
status: stable
---

# Output formats and exit codes

`FR-06` covers what a run reports, `FR-07` what it returns.

## Formats

`--format` selects one of four, `text` being the default:

* `text`: one line per diagnostic, path, line and column, severity,
  code, message; a summary line with file and finding counts last.
* `json`: a document with one object per diagnostic carrying code,
  message, range, severity, source path and an applied fix when present,
  plus a document-level summary. It parses as canonical JSON.
* `sarif`: the same diagnostics in the SARIF 2.1.0 shape, one result per
  diagnostic, with rule metadata from the plugins.
* `junit`: a JUnit XML report, one testcase per file, one failure per
  diagnostic, so a CI system can read the run as a test result.

All formats report the same diagnostics in the same order (see
[Output determinism](output-determinism.md)) with the same filters
applied; only the packaging differs.

## Exit codes

* `0`: the run completed and reported no diagnostics.
* `1`: the run completed and reported one or more diagnostics.
* `2`: the run did not complete because of a usage error, an invalid or
  unreadable configuration, a plugin load failure (bad path, SHA mismatch,
  contract violation) or an unresolvable diff base - the failure is
  described on stderr, never leaked as a report.

Exit code 0 and 1 are result codes; 2 is an error code, and it always
wins over a result code when both conditions apply.
