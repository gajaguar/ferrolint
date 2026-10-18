# Directory Update Log

## 2026-10-18

* **Optional type inference engine**: Added
  `decisions/optional-type-inference-engine.md` - why ty is postponed and
  the opt-in cargo feature planned for it.

## 2026-10-17

* **Native rules**: Added `usage/native-rules.md` - the Pylint rules the host
  runs itself, the text and format batch and its divergences.

* **Pylint configuration**: Extended `usage/configuration-file.md` with the
  Pylint files a run reads, their precedence and translation.

* **Pylint comparison**: Added `toolchain/pylint-compare.md` - how
  `make pylint-compare` measures parity with Pylint on a codebase.

* **Type inference query**: Added `decisions/type-inference-query.md` -
  the phase 2 `inference` interface and what the host infers.

* **Performance and releases**: Added `toolchain/ruff-upgrade.md`,
  `toolchain/release-builds.md`, `usage/plugins.md` and
  `decisions/packed-ast.md`.

* **Result cache and diff mode**: Added `usage/result-cache.md` and
  `usage/diff-mode.md`.

* **Report formats**: Added `usage/report-formats.md` - the four formats
  and their shared order.

* **Suppression comments**: Added `usage/suppression-comments.md` - the
  three comment forms, the line rule and the inert Pylint directives.

* **Writing a plugin**: Added `usage/writing-a-plugin.md` - the SDK, the
  sample plugin and its Pylint parity fixtures.

## 2026-10-16

* **Plugin sandbox environment**: Added
  `decisions/plugin-sandbox-environment.md` - how the host empties WASI
  and runs each check in a fresh, capped instance.

* **usage/**: New directory with `configuration-file.md` and
  `host-rules.md`: the keys a configuration takes and the `FL` codes the
  host reports.

* **Backlog**: Added `docs/backlog.md` - the eight implementation epics with
  their acceptance criteria.
* **Requirements bundle**: Added `docs/requirements/` - atomic FR and NFR
  notes, Acceptance criteria per area, and the AC index.
* **Initialization**: Initial scaffold (rust).
* **decisions/**: New directory with the decision record on building an
  extensible linter over a Ruff fork.
