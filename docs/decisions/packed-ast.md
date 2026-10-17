---
type: decision
title: Packed AST
description: Why contract 0.2.0 sends a file's AST as a list of u32 words and one string blob instead of a list of node records.
tags: [plugins, performance]
status: stable
---

# Packed AST

Contract 0.1.0 sent each file as `list<node>`, a record per node with
nested `range`/`position` records and two `option<string>` fields. With
ten plugins over a million lines, profiling showed the host spending most
of each check lowering that list into the plugin: wasmtime stores a
record field by field, and every string costs a call into the guest's
allocator.

## Decision

Contract 0.2.0 sends a `record ast` holding `nodes: list<u32>`, fourteen
words per node, and `strings: string`, one blob that names and details
point into as byte spans. A `list<u32>` and a `string` each cross as one
bulk copy. The host packs a file once and hands the same words to every
plugin.

The SDK decodes the words into `Node` views whose `name` and `detail`
borrow from the blob, so a plugin written against the SDK never sees the
packing. A plugin written against the raw contract decodes the layout the
WIT comment on `ast` documents.

## Consequences

* The cold run over the benchmark corpus went from 14.6 s to 6.2 s on
  four cores, together with the pooling allocator.
* The node layout is now part of the contract: adding a node field means
  a new contract version, and the SDK's kind table must follow the
  `node-kind` order, which a unit test checks.
