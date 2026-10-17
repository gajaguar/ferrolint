---
type: reference
title: Suppression comments
description: Which comment forms silence a finding, on which line, and which Pylint directives ferrolint leaves inert.
tags: [rules]
status: stable
---

# Suppression comments

A comment silences the findings whose code it lists, on its own line:

```python
import os  # ferrolint: disable=IMP001
import re  # pylint: disable=W0611, unused-import
import sys  # noqa: IMP001
```

* The three forms behave the same. `ferrolint:` and `pylint:` take a
  `disable=` list and their keyword is case-insensitive; `noqa:` takes a
  list after the colon. Several directives can share a comment.
* A listed code may be a rule's code, its Pylint msg-id or its Pylint
  symbol. A code no rule answers to suppresses nothing and is not an
  error.
* The comment applies to a finding that starts or ends on its line, so a
  multi-line finding can be silenced at either end. A `ferrolint:` or
  `noqa:` comment on a line of its own applies to nothing.
* A `pylint: disable=` on a line of its own covers from that line to the
  end of the innermost compound statement (`def`, `class`, `if`, `for`,
  `while`, `with`, `try`, `match`) that contains it, or to the end of the
  module. Placed before a def's or class's first statement (after any
  docstring), or before the module's first statement, it covers from the
  def or class line, or from line 1. Trailing a `def` or `class` line, it
  covers the whole definition; trailing any other line, only that line.
* A `pylint: disable-next=` covers the next line only.
* A bare `# noqa` suppresses nothing and is itself reported as `FL002`.
* An `FL001` plugin abort is never suppressed.

* A `disable=` list is comma-separated and ends at the first entry
  followed by prose; a `noqa:` list may also use spaces and ends at the
  first word that has no digit, `-` or `_`.

Pylint's other directives suppress nothing here: `enable=`, `skip-file`,
and `disable-all`.
