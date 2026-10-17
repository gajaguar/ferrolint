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
  multi-line finding can be silenced at either end.
* A bare `# noqa` suppresses nothing and is itself reported as `FL002`.
* An `FL001` plugin abort is never suppressed.

Pylint's other directives suppress nothing here: `disable-next=`,
`enable=`, and a `# pylint: disable=` on a line of its own, which Pylint
applies to the rest of the block. ferrolint's unit is always one line.
