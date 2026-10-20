# Directory Update Log

## 2026-10-19

* **plugins.md**: `singleton-comparison` words `C0121` like Pylint for quoted
  operands, operand order and the `bool(...)` suggestion.

* **native-rules.md**: Batch W completes `pylint.extensions.docparams` with
  `W9012`, `W9014`, `W9019`, `W9020` and `W9021`.

* **native-rules.md**: Batch W adds the return, raise and yield messages of
  `pylint.extensions.docparams`: `W9006`, `W9008`, `W9010`, `W9011` and
  `W9013`.

* **native-rules.md**: Added Batch W - the parameter messages of
  `pylint.extensions.docparams` (`W9005`, `W9015` to `W9018`), over a
  docstring parser for the Sphinx, Epytext, Google and NumPy styles.

* **native-rules.md**: Batch X adds `R0801` duplicate-code to the project
  phase, with the `min-similarity-lines` and `ignore-*` options.
* **configuration-file.md**: `duplicate-code` is no longer the example of a
  rule ferrolint does not have.

* **native-rules.md**: Batch X adds `E0401` import-error, `E0402`
  relative-beyond-top-level, `E0611` no-name-in-module and `R0401`
  cyclic-import, run as a project phase after the per-file pass.
* **result-cache.md**: The project rules keep their result in one stamped
  entry instead of the per-file ones.

* **native-rules.md**: Added Batch X with `W1116` isinstance-second-argument-not-valid-type,
  for literals, functions and instances that a name bound once stands for.

* **native-rules.md**: Batch V adds `W0632` unbalanced-tuple-unpacking and
  `W0644` unbalanced-dict-unpacking, for tuple, list, set and dict displays
  (also behind a name or a function of the file) and the views of a dict.

* **native-rules.md**: Batch V adds `E1111` assignment-from-no-return and
  `E1128` assignment-from-none, for the functions of the file that `W1114`
  resolves.

* **native-rules.md**: Added Batch V - `W0714` overlapping-except, on the
  builtin exception tree and the classes of the file; it needs
  `pylint.extensions.overlapping_exceptions`. `E0701` follows `builtins.X`,
  `socket.error` and the other names of a builtin exception.

* **native-rules.md**: `W0614` skips a module whose compiled `.so` sits next to
  its `.py` (astroid loads the compiled one and finds it empty), as Pylint does.

* **native-rules.md**: `W0614` no longer counts a read of a name that a
  function, class or comprehension binds itself as a use of the wildcard's name.

* **native-rules.md**: Added Batch O - `E4703`, `R0202`, `R0203`, `W0244` and
  `W3201`, the Pylint messages that Ruff has only as preview rules (ferrolint
  does not run Ruff, so no setting turns them on); `W3201` needs
  `pylint.extensions.dunder` in `load-plugins`.

* **native-rules.md**: Added Batch P - `W4901`, `W4902` and `W4904`, the
  deprecated modules, methods and classes of Pylint's tables, dumped by
  `make pylint-deprecated`.
* **native-rules.md**: Added Batch Q - `W4903`, `W4905` and `W4906`, the
  deprecated arguments, decorators and attributes of the same tables.

* **native-rules.md**: Added Batch S - `W0238`, `R0903`, `E0239`, `E0243` and
  `W0143`, private members, public methods and the rules that ask what an
  expression stands for, without type inference.
* **native-rules.md**: Added Batch R - `E1120`, `E1121`, `E1123`, `E1124`
  and `E1125`, the arity of a call, on the callees that `W1114` resolves.

* **native-rules.md**: Added Batch U - `E0110`, `E0240`, `E0244`, `W0239` and
  `W0240`, on abstract instantiation, inconsistent MROs, enumerations that
  already have members and `typing.final` classes and methods. A package
  directory is no longer a module root, so a sibling `collections.py` does not
  hide the standard library's, and `collections.abc` reads as
  `_collections_abc`.
* **native-rules.md**: Added Batch T - `W0222`, `W0233`, `W0236`, `W0246` and
  `R0901`, the override and ancestor-count messages that read the MRO of
  Batch G; removed the stale line that listed `W0237` and `E0203` as not
  ported.
* **suppression-comments.md**: A `skip-file`, `disable-all` or `disable=all`
  comment now ends the file: only the pragma messages and a syntax error
  remain, plugin findings included. **native-rules.md**: Batch N follows.

* **native-rules.md**: `R1727` and the other messages of the condition rules
  quote the code as astroid prints it, not as written.
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
* **native-rules.md**: Added Batch M - `R6101`, `R6102`, `R6103`, `R6105` and
  `W0141`, the `code_style` and `bad_builtin` extension messages; `R6105`
  is off until `enable` or `select` names it.
* **native-rules.md**: Added Batch N, the informational pragma messages
  `I0010`, `I0011`, `I0013`, `I0022` and `I0023`, off by default.
* **pylint-compare.md**: Added `SELECT`.
* **native-rules.md**: `R0902`, `W0201`, `E0202` and `E0203` drop an
  attribute outside the `__slots__` of the class and its ancestors, and
  `x = cls()` in a classmethod gives the class an attribute like `x = Model()`.
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
