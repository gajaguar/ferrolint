# Toolchain

Which layer — mise or an ecosystem package manager — installs which tool,
and why.

* [Toolchain layering rule](layering-rule.md) - mise bootstraps, the
  ecosystem's own package manager installs everything else.
* [Rejected install backends](rejected-install-backends.md) - the mise
  `npm:`/`pipx:` backends and pre-commit-managed environments this rules
  out.
* [Re-tag the notes](retag-notes.md) - run `make docs-retag`, review the
  dry run, then write the tags.
* [Ruff upgrade](ruff-upgrade.md) - moving every Ruff crate to a new tag
  in one change, and the current tag.
* [Release builds](release-builds.md) - the three release targets, their
  smoke check, and building one locally.
* [Pylint comparison](pylint-compare.md) - `make pylint-compare` lists the
  findings that ferrolint and Pylint do not share on a codebase.
