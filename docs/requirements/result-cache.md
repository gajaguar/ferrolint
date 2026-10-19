---
type: rule
title: Result cache
description: The run cache keyed on the file hash, the plugin hash and the configuration hash.
tags: [requirements, cache]
status: stable
---

# Result cache

`FR-09`: an unchanged file linted by unchanged plugins under an unchanged
configuration does not get parsed and walked a second time.

* The cache key is the triple hash of the file content, each plugin's
  WASM binary, and the effective configuration that changes results
  (select, ignore, per-file-ignores, suppressions options) - including
  the `options` sent to plugins, shared and per plugin, which a plugin's
  verdict may depend on.
  Any of the three changing invalidates the entry; nothing more or less
  does.
* A cache hit replays the stored diagnostics of that file, plugin for
  plugin, instead of parsing and running plugins; the report output is
  then indistinguishable from a cold run - determinism and filters are
  downstream of replay, never computed per run.
* The cache lives in a `.ferrolint-cache` directory in the working
  directory; `--no-cache` skips it entirely, and a run with a receipt
  that fails its own hash check re-lints rather than reporting a stale
  value.
* The cache never changes a verdict: it is a skip of work, not a
  notation. Two runs of the same tree, warm and cold, produce the same
  diagnostics in the same order by construction (see
  [Output determinism](output-determinism.md) for the hardening).
* Warm-cache runs are held to the same [performance
  budgets](performance-budgets.md) as a cold run, only faster.
