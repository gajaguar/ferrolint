---
type: decision
title: Rejected install backends
description: "Why mise's npm:/pipx: backends and pre-commit-managed tool environments were ruled out in favor of the layering rule."
tags: [toolchain]
status: stable
---

# Rejected install backends

Alternatives considered and rejected when establishing
[`layering-rule.md`](layering-rule.md):

* **mise `npm:` / `pipx:` backends** (e.g. `"npm:cspell" = "10"`) — resolve
  at install time with no transitive lockfile, so reproducible installs and
  `--frozen-lockfile` in CI are impossible.
* **pre-commit-managed tool environments** — pre-commit fetches its own
  copies of tools that the ecosystem manager already installs, at
  independently pinned versions; the two layers can drift and reach
  different verdicts on the same file.
