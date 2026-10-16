---
type: reference
title: Implementation backlog
description: The eight epics that implement ferrolint and the acceptance criteria each one closes.
tags: [requirements, testing]
status: stable
---

# Implementation backlog

The scaffold holds no linting logic. Each epic below is one GitHub issue and
closes the ignored tests in `crates/ferrolint/tests/acceptance.rs` listed
beside it; remove an `#[ignore]` only when its criterion passes.

- **E1** File discovery, parallel parse, config, exit codes. Closes AC-01 to
  AC-06, AC-19 to AC-21.
- **E2** WASM host, sandbox, plugin contract. Closes AC-07 to AC-09, AC-29,
  AC-30.
- **E3** Plugin SDK and a sample plugin. Closes AC-10..
- **E4** Suppression comments and Pylint code mapping. Closes AC-11 to AC-13,
  AC-18.
- **E5** Output formats and deterministic ordering. Closes AC-14 to AC-17,
  AC-31.
- **E6** Result cache and `--diff-base`. Closes AC-22 to AC-25..
- **E7** Benchmarks, budgets and release builds. Closes AC-26 to AC-28, AC-32
  to AC-34.
- **E8** Phase 2: type-inference query API (design first). Closes FR-11, none
  yet.
