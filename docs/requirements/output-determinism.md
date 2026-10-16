---
type: rule
title: Output determinism
description: Diagnostics always come out in one defined order, independent of threads, plugin order in configuration, or cache state.
tags: [requirements, output, performance]
status: stable
---

# Output determinism

`NFR-04`: the same input produces byte-identical reports.

* Diagnostic order is defined once, at the level of a total ordering on
  (source path, start offset, end offset, code): a run sorts into that
  order before it formats, after collect and filters, so parallel
  scheduling, plugin order or hash-table iteration cannot leak into it.
* Files run in parallel across the [rayon pool
  budget](performance-budgets.md) diagnostics may still arrive in any
  order, so results are gathered per file then sorted; per-file plugin
  ordering follows the order the configuration declares, not load
  order.
* The [result cache](result-cache.md) replay inserts into the same
  ordering, so a warm run and its cold twin produce the same bytes on
  the same tree.
* Output is stable two ways: two machines, run to run, produce the same
  diagnostics in the same order for the same source; and stable to
  the run's own inputs changing - reordering entries in the
  configuration file changes nothing unless the rules themselves
  change.

When a format needs document-level data (counts, run metadata), the
requirement is stable across runs for the same input, so a diff of two
runs is exactly the diagnostics that changed.
