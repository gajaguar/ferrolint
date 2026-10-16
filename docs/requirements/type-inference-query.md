---
type: rule
title: Type inference query
description: The phase 2 extension to the plugin contract - a type inference query API beyond v1's AST and scope stage.
tags: [requirements, plugins]
status: stable
---

# Type inference query

`FR-11`: after v1, the contract's scope query grows into a type-aware
query API. v1 does not ship it.

* Phase 1 is strictly pattern-first: AST plus scopes with bindings, so
  the v1 [contract](plugin-contract.md) is stable without inference. The
  type query is the named phase 2 extension.
* Phase 2 adds a query that, for a name or expression node, returns an
  inferred type summary (its name, its origin such as an import, class
  definition or call return, and whether the engine resolved it or
  gave up) instead of only name resolution.
* The type query lives in the same WIT interface as its own versioned
  export, optional for a plugin to call: an existing plugin runs
  unmodified when it ignores it, so phase 2 needs no contract break.
* Inference in phase 2 is best effort: the query results may return
  "unresolved" on dynamic patterns, and runners around plugins may
  filter on that. Plugin rules that only make sense when a type is
  resolved stay silent instead of reporting on an unresolved guess.
* Tests against a sample checker (one built for phase 2 or the phase 1
  sample extended) cover resolution successes and the silence path, so
  an inference regression cannot surface as a false-positive wave.

Until the type query lands, any config or contract field claiming it is
out of date with this bundle; remove it rather than stub it.
