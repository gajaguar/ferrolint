---
type: reference
title: Configuration file
description: The keys ferrolint.toml and the [tool.ferrolint] table accept, with how paths, globs and rule spellings resolve.
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

[[plugins]]                          # one table per plugin, in run order
path = "plugins/unused_import.wasm"
sha256 = "<the file's lowercase hex SHA-256>"

[per-file-ignores]
"tests/*.py" = ["W0611"]

[pylint-aliases]                     # who owns an alias two rules declare
W0611 = "IMP001"
```

* Relative paths and patterns resolve against the directory holding the
  configuration file.
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
