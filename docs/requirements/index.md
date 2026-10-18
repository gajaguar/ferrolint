# Requirements

The functional and non-functional requirements of ferrolint, split into
atomic notes: what a run does, what a plugin does, and what the
engineering constraints are. Acceptance criteria — the Given/When/Then
form — live in the sibling [acceptance](acceptance/index.md) directory,
indexed by [`ac-index.md`](ac-index.md).

## Functional requirements

* [Target selection](target-selection.md) - files and directories a run
  covers, honouring gitignore and configured excludes.
* [Configuration](configuration.md) - `[tool.ferrolint]` or
  `ferrolint.toml`: plugins with path and SHA, select, ignore,
  per-file-ignores.
* [Plugin contract](plugin-contract.md) - the WIT interface: events,
  scope and binding queries, diagnostics with code, message, range,
  severity and optional fix.
* [Plugin SDK and sample plugin](plugin-sdk.md) - the Rust SDK crate and
  the sample porting one real Pylint checker.
* [Inline suppressions](inline-suppressions.md) - `ferrolint: disable=`,
  `pylint: disable=` and `noqa:` in comments.
* [Output formats and exit codes](output.md) - text, json, sarif and
  junit out; exit 0 clean, 1 findings, 2 error.
* [Diff mode](diff-mode.md) - `--diff-base REF` lints only changed
  files.
* [Result cache](result-cache.md) - keyed on file, plugin and
  configuration hashes.
* [Pylint code mapping](pylint-code-mapping.md) - msg-ids and symbols
  resolve to plugin codes.
* [Type inference query](type-inference-query.md) - the type inference
  query extension to the contract (interface added in contract 0.2.1).

## Non-functional requirements

* [Performance budgets](performance-budgets.md) - 10x Pylint, one million
  lines under ten seconds, warm caches, 100 ms startup, 2 GB ceiling.
* [Plugin sandboxing](plugin-sandboxing.md) - empty WASI, denied imports,
  per-file fuel and epoch budgets.
* [Output determinism](output-determinism.md) - one total order, from any
  schedule, any cache state.
* [Platforms and release gate](platforms-and-release-gate.md) - one
  static binary per target, one pinned Ruff tag, `make check` and
  `make test` green.

## Acceptance criteria

Every requirement is acceptance-tested through criteria with stable IDs;
each area note collects the criteria for one concern.

* [Configuration](acceptance/configuration.md) - configuration load,
  precedence, plugin SHA verification, select and ignore filtering.
* [Plugins](acceptance/plugins.md) - contract events, queries,
  diagnostics, sample plugin parity.
* [Suppressions](acceptance/suppressions.md) - the three comment forms.
* [Output](acceptance/output.md) - the four formats and code aliases.
* [Exit codes](acceptance/exit-codes.md) - 0, 1 and 2 paths.
* [Diff mode](acceptance/diff-mode.md) - ref resolution and changed-file
  selection.
* [Cache](acceptance/cache.md) - replay and invalidation.
* [Performance](acceptance/performance.md) - budgets under load.
* [Sandbox](acceptance/sandbox.md) - no escape, no hang.
* [Determinism](acceptance/determinism.md) - byte-identical reruns.
* [Platforms](acceptance/platforms.md) - targets, pinning, gate.
