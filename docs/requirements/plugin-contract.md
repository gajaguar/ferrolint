---
type: rule
title: Plugin contract
description: The WIT (component model) interface a WASM plugin implements - events, queries and diagnostics - independent of the SDK.
tags: [requirements, plugins]
status: stable
---

# Plugin contract

`FR-03`: a plugin is a WASM component built for `wasm32-wasip2` that
implements a versioned WIT interface, loadable with or without the Rust
SDK.

* Per linted file, the host calls:
  * file lifecycle events: the plugin receives file metadata plus the
    parsed AST,
  * AST visitor events: nodes reported in source order so a checker can
    walk the tree the way a Python observer visits it,
  * scope and binding queries: which bindings are visible at a node, and
    what a name resolves to.
* The file metadata also carries the configuration's `options`, a list of
  (key, value) text pairs sorted by key, as written: `("max-args", "3")`
  for Pylint's `max-args=3`. The host sends the same list with every file
  and ignores its meaning, and a changed option changes the
  [result cache](result-cache.md) key, since a plugin's verdict may
  depend on it.
* Two more queries, about the current file and the calling plugin, come
  from contract 0.3.1:
  * `comments.comment-ranges` returns the ranges of the file's comments in
    source order, each from its `#` to the end of the line; the text is
    the source between the offsets, so a `#` inside a string is not one.
  * `settings.plugin-options` returns the plugin's own options, the
    `options` table of its `[[plugins]]` entry, as (key, value) text pairs
    sorted by key; it is empty for a plugin without the table and for a
    bundled plugin.
* The host owns the AST; the interface exchanges only serializable data,
  so a plugin never holds a host pointer between event calls.
* Per diagnostic, the plugin returns a diagnostic record with exactly:
  `code`, `message`, `range` (start and end byte offsets plus line and
  column), a `severity` of `error`, `warning` or `info`, and an optional
  `fix` (a replacement text edit).
* The plugin never mutates the source file; `fix` data is a suggestion
  the host applies or reports, never evaluated on the plugin side.
* The host compiles and instantiates each plugin once per run and reuses
  it for every file (see [Plugin sandboxing](plugin-sandboxing.md) and
  [Performance budgets](performance-budgets.md)).

Contract 0.3.0 added `options` to the file record. That changes the
record's shape, so a host of 0.3.0 refuses a plugin built against 0.2.x
(rebuild it with SDK 0.3.0), unlike the additive 0.2.1.

The type inference query is a separate interface, added in contract 0.2.1
(see [Type inference query](type-inference-query.md)).

The comment and settings queries are two more separate interfaces, added
in contract 0.3.1 as 0.2.1 added inference: they change no record, so a
host of 0.3.1 still loads a plugin built against 0.3.0, which imports
nothing of them. A plugin built with SDK 0.3.1 needs a host of 0.3.1.

## Why the plugin's own options are a query

The per-plugin options are not another field of the `file` record: that
would change the record's shape again and refuse every built plugin, as
0.3.0 did. They live in the plugin's `[[plugins]]` entry because the entry
already identifies the plugin (its path and SHA-256), the host reads it
before any file, and the text-valued map matches the shared `[options]`.
The result cache keys on them for the plugins a file runs, so a changed
value re-runs those plugins and never replays a stale verdict.
