---
type: reference
title: Host rules
description: The FL rule codes ferrolint reports itself, without a plugin, and when each one fires.
tags: [rules]
status: stable
---

# Host rules

The linter owns three rules. They select, ignore and suppress like any
plugin rule.

| Code    | Name             | Severity | Fires when                                                        |
| :------ | :--------------- | :------- | :---------------------------------------------------------------- |
| `FL001` | `plugin-aborted` | error    | a plugin trapped, or hit its fuel or time cap, on a file          |
| `FL002` | `bare-noqa`      | warning  | a `# noqa` comment names no codes, so it suppresses nothing       |
| `FL003` | `syntax-error`   | error    | the file does not parse, or is not UTF-8; Pylint alias `E0001`    |

A file with a syntax error still gets its comment checks, but no plugin
runs on it.

`FL003` follows the Ruff parser, which accepts a few things CPython rejects, so
Pylint's `E0001` can have no counterpart. One is iterable unpacking in a
comprehension (`[*x for x in y]`): the repository's own
`crates/ferrolint/src/native/fixtures/versions.py` contains it on purpose, to
exercise the version checks, and is the only file where the two linters differ
on this repository.
