---
type: rule
title: Performance budgets
description: The time and memory budgets a run must meet - vs. Pylint, on huge codebases, cold and warm.
tags: [requirements, performance]
status: stable
---

# Performance budgets

The budgets a run must meet (`NFR-01`, `NFR-02`, `NFR-05`); every number
below is a hard ceiling, not a target, and a regression past one is a
bug.

## End-to-end speed

* On the same custom checks ferrolint runs, a full run is at least ten
  times faster than Pylint running those checks (`NFR-01`).
* A corpus of one million lines with ten plugins, cold cache, eight
  cores, completes in under ten seconds wall clock (`NFR-01`).
* The same workload with a warm [result cache](result-cache.md) is at
  least five times faster than its cold run again (`NFR-01`).

## Startup and parallelism

* From process start to the first file being parsed is under 100
  milliseconds (`NFR-02`), regardless of how many plugins the
  configuration names.
* Files fan out across a rayon pool sized to the available cores, and
  the fan-out keeps all cores busy for the length of the run (`NFR-02`).
* Each WASM plugin compiles exactly once per run and shares that
  compiled instance across all files; recompiling per file doubles the
  run time at the scale above (`NFR-02`; the contract carries the
  compile once promise into the plugins).

## Memory

* Peak resident memory over the one-million-line corpus stays under 2 GB
  (`NFR-05`), with the cold budget above it running to completion and no
  cache spill to disk.

Budgets below the [sandboxes](plugin-sandboxing.md) are not prorated for
a plugin run: a run whose plugin misses a budget still misses the
budget.
