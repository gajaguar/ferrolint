---
type: rule
title: Type inference query
description: The type inference query, an extension to the plugin contract - a type-aware query API beyond the AST and scope stage.
tags: [requirements, plugins]
status: stable
---

# Type inference query

`FR-11`: the contract's scope query grows into a type-aware query API.
It is the type inference query extension to the plugin contract (interface
added in contract 0.2.1).

* The base contract is pattern-first: AST plus scopes with bindings, so
  the [contract](plugin-contract.md) works without inference. The type
  query is an additional interface on top of it.
* The query, for a name or expression node, returns an inferred type
  summary (its name, its origin such as an import, class definition or call
  return, and whether the engine resolved it or gave up) instead of only
  name resolution.
* The type query lives in the same WIT interface as its own versioned
  export, optional for a plugin to call: an existing plugin runs unmodified
  when it ignores it, so the extension needs no contract break.
* Inference is best effort: the query results may return "unresolved" on
  dynamic patterns, and runners around plugins may filter on that. Plugin
  rules that only make sense when a type is resolved stay silent instead of
  reporting on an unresolved guess.
* Tests against a sample checker (the `not-callable` plugin, or a pattern
  sample extended with inference) cover resolution successes and the silence
  path, so an inference regression cannot surface as a false-positive wave.
