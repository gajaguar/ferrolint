---
type: rule
title: Acceptance criteria - cache
description: Given/When/Then criteria for the result cache key and invalidation, mapped to FR-09.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: cache

## AC-24: A warm run replays unchanged files (FR-09)

* Given a run over a tree, followed by a second run over the identical
  tree, plugins and configuration,
* when the second run compares the [cache keys](../result-cache.md),
* then unchanged files skip parsing and plugin execution entirely and
  replay their stored diagnostics, while the report is byte-identical
  to the first run's (determinism holds through the cache).

## AC-25: Each key-part change invalidates (FR-09)

* Given a warm cache in place, and three run variants that each change
  exactly one key part - the byte content of one linted file, one
  plugin's binary, and the configuration (for example the `ignore`
  list),
* then each variant re-lints the files the change touches, keeps the
  stored diagnostics for files the change does not touch, and its
  report equals a cold run over the same variant - so a stale entry can
  never surface.

Given the byte-identical requirement, a cache that changes a verdict,
an order or a count is a failure even when it is faster.
