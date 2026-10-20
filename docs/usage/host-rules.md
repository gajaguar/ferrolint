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

Known divergence from CPython: the Ruff parser, pinned at tag 0.17.1 and
still the same on its main branch, rejects a `raise` followed by `;` and
another statement (`raise;x = 1`), which CPython accepts, so `FL003` fires
on such a file. Lifting the pin fixes it once Ruff does.
