---
type: rule
title: No CLAUDE.md check
description: make claude-md-check fails when a CLAUDE.md exists anywhere in the repository, because AGENTS.md is the only agent instructions file.
tags: [agents]
status: stable
---

# No CLAUDE.md check

`AGENTS.md` is the only agent instructions file; a `CLAUDE.md` beside it
becomes a second copy that drifts. `make claude-md-check` fails, listing each
path, when `git ls-files --cached --others --exclude-standard` reports a file
named `CLAUDE.md` at any depth. It runs as part of `make check`.

The scan covers tracked files and untracked files that are not git-ignored,
so a stray file is caught before it is committed, and a git-ignored one
(such as under `node_modules/`) is not.

To clear a failure, fold the file into `AGENTS.md` section by section and
delete it.
