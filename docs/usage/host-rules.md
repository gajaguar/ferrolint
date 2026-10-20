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

`FL003` words its message as Pylint's `E0001` does:
`Parsing failed: '<error> (<module>, line N)'`, where `<module>` is the
dotted name the file imports under (its path below the nearest directory
without `__init__.py`) and `N` is the line the finding reports.

Known divergences from Pylint:

* `<error>` is the Ruff parser's text, not CPython's: `missing closing quote
  in string literal` where Pylint says `unterminated string literal (detected
  at line 35)`.
* The line can differ: Ruff reports an unclosed bracket at the end of the file,
  where CPython reports the line of the bracket.
* For an `__init__.py`, `<module>` is the package name (`pkg`), as the import
  system names it; `pylint .` names the file `pkg.__init__`.

`FL003` follows the Ruff parser, which accepts a few things CPython rejects, so
Pylint's `E0001` can have no counterpart. One is iterable unpacking in a
comprehension (`[*x for x in y]`): the repository's own
`crates/ferrolint/src/native/fixtures/versions.py` contains it on purpose, to
exercise the version checks, and is the only file where the two linters differ
on this repository.

Known divergence from CPython: the Ruff parser, pinned at tag 0.17.1 and
still the same on its main branch, rejects a `raise` followed by `;` and
another statement (`raise;x = 1`), which CPython accepts, so `FL003` fires
on such a file. Lifting the pin fixes it once Ruff does.
