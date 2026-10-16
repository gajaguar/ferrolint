---
type: rule
title: Acceptance criteria - determinism
description: Given/When/Then criteria for byte-identical reruns, mapped to NFR-04.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: determinism

## AC-31: Same input, same output, any schedule (NFR-04)

* Given one tree with one configuration that yields findings under
  several plugins, run 50 times with varying thread counts and against
  a warm cache on the second half,
* when the reports are compared,
* then every run's report (text and json) is byte-identical, including
  plugin-internal iteration in the [declared configuration
  order](../output-determinism.md), byte-level across two machines
  built from the same pin set.
