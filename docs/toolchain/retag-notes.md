---
type: playbook
title: Re-tag the notes
description: Run make docs-retag, review the dry run, then make docs-retag-apply writes the tags and validates docs/.
tags: [documentation, makefile]
status: stable
---

# Re-tag the notes

Re-tag after a note changes enough to shift its subject, or after the
[tag vocabulary](../conventions/tag-vocabulary.md) changes.
`tools/docs-retag.py`, which `uv` runs with its dependencies declared inline,
turns each row of the vocabulary into a yes/no question that TypeSafe's Jev
answers for every note. A note takes a tag whose probability is above 0.5; a
tag outside the vocabulary stays, and a vocabulary tag Jev no longer
supports is removed.

1. Check that the TypeSafe profile works: `uvx --from typesafe-unofficial-cli
   typesafe auth status`.
2. Run `make docs-retag`. It builds one question per tag and one state per
   note into `.cache/docs-retag/`, asks Jev and prints a dry run. Each run is
   billed, one request per note, so run it on purpose, not from `make check`.
3. Review the changes marked `*` and the answers between 0.3 and 0.7, which
   the dry run lists as borderline.
4. Run `make docs-retag-apply`. It writes the tags from the last run, without
   asking again, then runs `make docs-lint`.

`.cache/` is git-ignored: the answers belong to a run, not to the repository.
