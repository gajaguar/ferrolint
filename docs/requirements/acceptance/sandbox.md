---
type: rule
title: Acceptance criteria - sandbox
description: Given/When/Then criteria for the empty-WASI environment and the per-file fuel and epoch budgets, mapped to NFR-03.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: sandbox

## AC-29: A plugin touches nothing it should not (NFR-03)

* Given a plugin exercising, in turn, a filesystem open, a socket open,
  an environment read, and an import of a host-disabled builtin beyond
  the [contract](../plugin-contract.md),
* then each attempt fails inside the instance - none reaches the host
  or the machine - the failure to load or run is reported, and the rest
  of the run's files still lint.

## AC-30: A runaway plugin cannot hang a run (NFR-03)

* Given a plugin whose visitor loops forever on a planted fixture, and
  a per-file fuel or epoch budget the host picks,
* then the plugin's work on that file aborts at the cap, that file's
  report carries the termination as
  [exit code 2](exit-codes.md) would produce, and the run completes
  over the remaining files within its [time
  budget](performance.md), never tuned by plugin request.
