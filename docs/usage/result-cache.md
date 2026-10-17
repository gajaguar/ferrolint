---
type: reference
title: Result cache
description: What .ferrolint-cache/ stores, what invalidates an entry, and how a warm run skips parsing, plugins and compiling.
tags: [cache]
status: stable
---

# Result cache

A run keeps its results in `.ferrolint-cache/` in the working directory;
`--no-cache` neither reads nor writes it. Add the directory to
`.gitignore`. `--stats` prints how many files came from the cache.

* A file's entry is keyed on its bytes, the rules that report for it
  (after `select`, `ignore` and `per-file-ignores`), the `pylint-aliases`
  table and the linter's version. Inside, each plugin's diagnostics are
  stored under the SHA-256 of the plugin binary.
* A file whose entry holds every plugin it needs replays without being
  parsed. Otherwise the run parses it, runs only the plugins the entry
  lacks and rewrites the entry. Changing one plugin reruns that plugin
  alone.
* A plugin's name and rules are stored under its SHA-256 too, so a run
  that replays every file compiles no plugin.
* An entry carries a checksum of its own content; one that fails it, or
  does not parse, counts as absent. A plugin abort is never stored, so the
  next run tries again.
* Replay feeds the same suppression, filtering and ordering steps as a
  cold run, so the report is byte-identical. Deleting the directory is
  always safe.
