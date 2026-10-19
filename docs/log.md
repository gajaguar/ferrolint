# Directory Update Log

## 2026-10-19

* **pylint-compare.md**: Documented `MSG=1`, which compares the message as
  well as the line, with whitespace collapsed.
* **native-rules.md**: `W0614` lists the public names of the standard
  library's C and builtin modules (`math`, `itertools`) from a generated table.
* **Plugin comment position**: Contract 0.4.0 adds `comments.comment-list`
  with the `own-line` flag. Updated `requirements/plugin-contract.md` and
  `usage/writing-a-plugin.md`.

* **native-rules.md**: Added Batch L - `C1803`, `C1804`, `C1805`, `E1143` and
  `E1144`, on literals and names bound once to one; `C1804` and `C1805` are off
  unless selected or enabled, as in Pylint.
* **native-rules.md**: Added Batch K - `W0149`, `C3401`, `R3501`, `R5601` and
  `R6106`, the Pylint extension messages that one walk decides, gated on
  `load-plugins`.
* **native-rules.md**: Added Batch L - `R6101`, `R6102`, `R6103`, `R6105` and
  `W0141`, the `code_style` and `bad_builtin` extension messages; `R6105`
  is off until `enable` or `select` names it.
* **native-rules.md**: `W0614` names the module as written and leaves out a
  name an earlier import or star already bound.
* **native-rules.md**: `W0614` now sees the names of `match` captures,
  walruses and `global` assignments, leaves out `from __future__` names, lists
  `from` imports last and matches module names by case.

## 2026-10-18

* **pylint-compare.md**: Described the comparison of configuration
  findings (`.pylintrc:1:E0015`) and the `E0013` path divergence.
* **native-rules.md**: Added `W0401` wildcard-import next to `W0614`, with
  `allow-wildcard-with-all`, and the cases of `W0614` checked against Pylint
  (private names, a star in a function, redefined names, nested stars).
* **Plugin comments and options**: Contract 0.3.1 adds the comment-ranges and
  per-plugin options queries. Updated `requirements/plugin-contract.md` (with
  why the options are a query), `requirements/plugin-sdk.md`,
  `requirements/configuration.md`, `requirements/result-cache.md`,
  `usage/configuration-file.md`, `usage/writing-a-plugin.md`,
  `usage/result-cache.md` and `usage/plugins.md`.
* **Out-of-tree plugin**: Added `usage/out-of-tree-plugin.md` - building a
  plugin from another repository against the SDK by revision.
* **Roadmap notes**: Retired `backlog.md` and
  `optional-type-inference-engine.md` from the public docs; the implementation
  backlog and the optional type inference engine decision are no longer part
  of the bundle.

* **native-rules.md**: Added Batch I - the unsupported-version rules
  `W2601` to `W2607`, `E1003` bad-super-call and `E0108`
  duplicate-argument-name, with their divergences.
* **native-rules.md**: Added Batch J, the Pylint configuration and pragma
  messages (`E0011` to `E0015`, `R0022`) and the messages left not ported.
* **native-rules.md**: Moved `W0614` unused-wildcard-import from not ported
  to implemented, with its divergences.

* **suppression-comments.md**: Added the pragma spellings Pylint accepts and
  the module-level `too-many-lines` rule.
* **release-builds.md**: Added the section on installing from source with
  the `bundle` feature.
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
