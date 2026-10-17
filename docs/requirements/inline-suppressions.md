---
type: rule
title: Inline suppressions
description: Code-level suppressions recognized in comments - ferrolint's own form, Pylint's and noqa.
tags: [requirements]
status: stable
---

# Inline suppressions

`FR-05`: a Python source can silence a specific code on a specific line,
with the author choosing which of three spellings to use.

* On any line, a comment adds one suppressions set for that line:
  `# ferrolint: disable=CODE`, `# pylint: disable=CODE` (callsign case
  aside, spelled exactly `pylint:` in the comment) and
  `# noqa: CODE`.
* All three honor the same `disable=` payload: a comma-separated list of
  codes, which may be plugin codes or Pylint aliases (see
  [Pylint code mapping](pylint-code-mapping.md)).
* A suppression silences a diagnostic whose range ends on that comment's
  line; a diagnostic on another line is untouched, and a suppressions
  comment for an unknown code is inert, not an error.
* A bare (payload-less) `# noqa` is linted as a diagnostic of its own by
  the suppression checker itself; no plugin is run under it.
* Spread over lines: `ferrolint:` and `noqa:` stay on one line. A
  `pylint: disable=` spans a block as Pylint does: its own line covers to
  the end of the enclosing compound statement or module, and a trailing
  one on a def or class line covers the whole definition. A
  `pylint: disable-next=` covers the next line. A diagnostic is silenced
  when its first or last line is covered.

Running with a suppressions-matching file twice yields identical output:
the removed diagnostics are deducted before format-specific ordering, so
a suppressed finding cannot resurface in JSON or SARIF.
