---
type: rule
title: Acceptance criteria - performance
description: Given/When/Then criteria for the speed, startup and memory budgets, mapped to NFR-01, NFR-02 and NFR-05.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: performance

## AC-26: Full-run budgets on a million lines (NFR-01)

* Given a one-million-line corpus with ten plugins, run cold on eight
  cores, with the same rule set handed to Pylint as the base,
* when both finish,
* then ferrolint's wall clock is at least one tenth of Pylint's on the
  same checks and under ten seconds absolute, and the same workload
  with a warm cache finishes at least five times faster than the cold
  run again.

## AC-27: Peak memory stays under the ceiling (NFR-05)

* Given the corpus and cold full run of AC-26,
* when the run completes,
* then peak resident RSS is below 2 GB, measured by the benchmark
  harness alongside the clocks, so the [speed
  budgets](../performance-budgets.md) never hide a memory blow-up.

## AC-28: Startup, compile-once and parallelism hold (NFR-02)

* Given a run starting with a configuration naming several plugins,
* then time to first parsed file is under 100 milliseconds from process
  start, each plugin's WASM compiles exactly once across the whole run
  (one compile counted per plugin), and during the file walk all eight
  cores carry work for the run's duration - the parallelism claim is
  not a core-scheduling accident.
