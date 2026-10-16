---
type: rule
title: Enforcing commits and branches
description: A pre-commit hook checks the commit being written and the current branch name; make commits-check re-validates the whole range in CI.
tags: [git]
status: stable
---

# Enforcing commits and branches

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)
and branch names follow [Conventional Branch](https://conventionalbranch.org/).
A commit type may be any Conventional Commits type (`docs`, `build`, `ci`,
...), but a branch type is one of `feat` (or `feature`), `fix` (or `bugfix`),
`hotfix`, `release`, or `chore`: documentation and dependency work uses
`chore/`, so `docs/` is not a valid branch prefix.
Two layers enforce both, via
[conventional-git](https://github.com/gajaguar/conventional-git):

* **Local**: `.pre-commit-config.yaml`'s `conventional-commit-msg` hook
  checks the message being written (`commit-msg` stage);
  `conventional-branch-name` checks the current branch name (`pre-commit`
  and `pre-push` stages).
* **CI**: `make commits-check` re-validates every non-merge commit in
  `$(BASE)..HEAD` and the branch name, since a local hook can be bypassed with
  `--no-verify`. It runs as part of `make check`.

Merge commits are skipped: their headers are generated (`Merge pull
request #N from ...`, `Merge branch 'main' into ...`) and are not Conventional
Commits, while the commits they bring in are still validated.

Dependabot always names its branches `dependabot/<ecosystem>/<dependency>`,
which is not a Conventional Branch type, and its prefix can't be changed.
`conventional-git` therefore accepts names that start with `dependabot/` or
`renovate/` (1.1.0 and later), in the hook and in `make commits-check`
alike; the commit messages are still validated, and
`.github/dependabot.yml` sets their `ci`/`chore` prefixes.
