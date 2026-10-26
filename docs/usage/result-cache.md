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

* A file's entry is keyed on everything its results depend on: its bytes,
  its path and module name, the rules that report for it (after `select`,
  `ignore` and `per-file-ignores`), `py-version`, the options sent to
  plugins (the shared `[options]` and each plugin's own `options`) and the
  linter build (its version and executable). Inside, each plugin's
  diagnostics are stored under the SHA-256 of the plugin binary, and
  outputs of plugins no longer configured are dropped when the entry is
  rewritten. It also records a stamp of each directory listed and file read
  from another module while the native rules resolved an import (`E1120` of a
  function of another module, say); the entry replays only while every stamp
  still holds.
* A file whose entry holds every plugin it needs replays without being
  parsed. Otherwise the run parses it, runs only the plugins the entry
  lacks and rewrites the entry. Changing one plugin reruns that plugin
  alone.
* A plugin's name and rules are stored under its SHA-256 and the linter
  build, so a run that replays every file compiles no plugin. A plugin
  that then fails to compile stops the run with exit code 2.
* An entry carries a checksum of its own content; one that fails it, or
  does not parse, counts as absent. A plugin abort is never stored, so the
  next run tries again.
* Replay feeds the same suppression, filtering and ordering steps as a
  cold run, so the report is byte-identical. Nothing evicts old entries;
  deleting the directory is always safe.
* The project rules (`E0401`, `E0402`, `E0611`, `R0401`, `R0801`) depend on
  files other than the one reported, so they are not in a file's entry. They run
  after the per-file pass on every run, and their findings are stored as one
  project entry, keyed on every linted file's bytes, path and module name, the
  rules that report for it, the options, `py-version`, `VIRTUAL_ENV` and
  `PYTHONPATH`. The entry also records a stamp of each directory listed and file
  read while resolving imports (its entries' names, or a file's size and
  modification time); a run replays the entry only while every stamp still
  holds, so installing a package or adding a module on the search path reruns
  the phase. A run that enables none of these rules does not run the phase.
  Under `--diff-base` the key still covers every file of the targets, not the
  changed ones, so a diff run and a full run share the entry; the reduction
  to the changed files happens after it is read.
