---
type: decision
title: Toolchain layering rule
description: mise installs what bootstraps an ecosystem or belongs to none; the ecosystem's own package manager installs everything else.
tags: [toolchain]
status: stable
---

# Toolchain layering rule

> **mise installs what bootstraps an ecosystem or belongs to none. The
> ecosystem's package manager installs everything else.**

A tool declared in two layers can drift between them — `make check` and the
git hook could then reach different verdicts on the same file, the exact
class of drift this rule exists to prevent.

| Tool                                | Where                                      | Why                                                |
| :---------------------------------- | :----------------------------------------- | :------------------------------------------------- |
| node, pnpm, uv                      | `mise.toml`                                | bootstrap: nothing else can install them           |
| checkmake                           | `mise.toml`                                | Go binary, no ecosystem in this repo               |
| pre-commit                          | `mise.toml`                                | meta-tool; must run before any ecosystem is set up |
| cspell, markdownlint-cli2           | `package.json`                             | Node dev deps, lockfile-managed                    |
| okflint, typesafe                   | `uvx` in the `Makefile`                    | Python tools, run on demand, pinned in the target  |

See [`rejected-install-backends.md`](rejected-install-backends.md) for the
alternatives this decision ruled out.
