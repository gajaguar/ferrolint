---
type: reference
title: Configuration file
description: The keys ferrolint.toml and the [tool.ferrolint] table accept, how Pylint's own configuration is read instead, and how paths, globs and rule spellings resolve.
tags: [config]
status: stable
---

# Configuration file

A run reads `ferrolint.toml` in the working directory; without one it reads
the `[tool.ferrolint]` table of `pyproject.toml`. `--config PATH` names the
file instead, read as a `pyproject.toml` when it has that name. Both forms
take the same keys, and an unknown key is an error:

```toml
select = ["FL002", "W0611"]          # default: every known rule
ignore = ["FL003"]                   # beats select
exclude = ["vendor/", "*_gen.py"]    # gitignore patterns, on top of .gitignore
py-version = "3.14"                  # default: none; MAJOR.MINOR

[[plugins]]                          # external plugins only, in run order
path = "plugins/unused_import.wasm"
sha256 = "<the file's lowercase hex SHA-256>"
options = { markers = "TODO, FIXME" }  # this plugin's own options, as text

[per-file-ignores]
"tests/*.py" = ["W0611"]

[pylint-aliases]                     # who owns an alias two rules declare
W0611 = "IMP001"

[options]                            # sent to every plugin, as text
max-args = "3"
```

* Relative paths and patterns resolve against the directory holding the
  configuration file.
* The default `select` covers every known rule, including the bundled
  plugins of a release binary (see [Plugins](plugins.md)); no `[[plugins]]`
  table is needed for them. `[[plugins]]` names external plugins only, and
  a bundled one repeated there is a duplicate rule code error.
* A `[[plugins]]` entry may carry an `options` table of text values that
  only that plugin reads, unlike the top-level `[options]`, which every
  plugin receives. Write it inline, or as `[plugins.options]` right after
  the entry. A bundled plugin takes none, and a Pylint file cannot name
  plugins, so these options need a `ferrolint.toml` or
  `[tool.ferrolint]`. Changing one re-runs that plugin on every file.
* `exclude` uses gitignore syntax: `vendor/` matches a directory anywhere,
  `/vendor/` only at the root. It filters what a directory walk discovers;
  a file named on the command line is always linted.
* A `per-file-ignores` pattern is a glob over the path relative to that
  directory; a pattern without `/` also matches the file name alone.
* `select`, `ignore` and `per-file-ignores` accept a rule's code, its
  Pylint msg-id or its Pylint symbol. A spelling no rule answers to is an
  error, so a typo fails the run instead of disabling a check.
* When two plugins declare the same Pylint msg-id or symbol, the run stops
  until `pylint-aliases` names the rule that owns it; the other rule keeps
  its code and its remaining aliases.

## Pylint configuration

With neither `ferrolint.toml` nor `[tool.ferrolint]`, a run reads the first
Pylint source that exists, in Pylint's order, and ignores the rest:

* `.pylintrc`, then `pylintrc` (INI, every section).
* `[tool.pylint.*]` in `pyproject.toml`.
* `setup.cfg`, then `tox.ini`, only their `[pylint.*]` sections.

Sources never merge. `--config PATH` with a file named like one of these is
read as Pylint configuration. The directory holding the file is the root of
relative paths.

* `disable` and `enable` entries are applied in file order; within one
  file, in the order the options appear. `disable` turns rules off and
  `enable` turns them back on. An entry is a ferrolint code, a Pylint
  msg-id or symbol, a category letter (`C`, `R`, `W`, `E`, `F`, `I`: the
  rules whose msg-id starts with it) or `all`.
* An entry naming no rule ferrolint has (`fixme`, a
  checker name) is dropped without an error, unlike `ignore`.
* `py-version` sets the target Python version; `ignore` (Pylint's base
  names, like `CVS`) is added to `exclude`.
* Every other option is accepted so a Pylint file never fails a run, and
  goes to the plugins as an option (below). `load-plugins`, `ignore-paths`
  and `ignore-patterns` have no effect of their own; the last two are
  regular expressions and are not supported yet.
* `[options]` and those other Pylint options reach every plugin on every
  file, as written, with kebab-case keys; the host rules ignore them. A
  Pylint file cannot name plugins, so a plugin that reads options needs a
  `ferrolint.toml` carrying both `[[plugins]]` and `[options]`.
* In `pyproject.toml` every `[tool.pylint.*]` table is flattened; arrays
  join with `,` and booleans become `yes` or `no`.
