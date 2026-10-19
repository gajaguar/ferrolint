---
type: reference
title: Native rules
description: The Pylint rules the host runs itself, by batch, with the cases each one leaves out compared with Pylint.
tags: [rules, pylint]
status: stable
---

# Native rules

A native rule is a Pylint rule that the host runs over the syntax tree and
the scope model of the file, with no plugin. Its code is the Pylint msg-id,
so `C0209` and `consider-using-f-string` name the same rule, and its message
is Pylint's own text, so the output compares line by line.

Native rules are active by default like every other rule. They select,
ignore and suppress like a plugin's: with `select`, `ignore`, a
`# pylint: disable=` comment or `# noqa: C0209`. They read the Pylint
options that the configuration carries (see
[the configuration file](configuration-file.md)), such as
`logging-modules`, the way Pylint reads them. A file with a syntax error gets
no native rule.

`make pylint-compare` measures a batch against Pylint on a codebase; see
[the comparison](../toolchain/pylint-compare.md).

## Batch A: text and format

* `C0209` `consider-using-f-string`: `"...".format(...)` and `"..." % x`.
* `W1300` `bad-format-string-key`: a `%` format with named fields given a
  dict literal with a key that is not a string.
* `W1307` `invalid-format-index`
* `W1308` `duplicate-string-formatting-argument`
* `E0119` `misplaced-format-function`: `print("{}").format(1)`.
* `W1402` `anomalous-unicode-escape-in-string`: `\u`, `\U` and `\N` in a
  byte string.
* `E1200` `logging-unsupported-format` and `E1201` `logging-format-truncated`;
  they read `logging-modules` and `logging-format-style`.
* `C2503` `bad-file-encoding`: a coding comment that is not UTF-8.
* `E2511` `invalid-character-carriage-return`: a `\r` inside a line.
* `C0327` `mixed-line-endings` and `C0328` `unexpected-line-ending-format`;
  the second reads `expected-line-ending-format`.

Not ported:

* `W1306` `missing-format-attribute` needs the attributes of an inferred
  type, which the host does not have.
* `E2501` `invalid-unicode-codec`: Pylint cannot reach it, since Python
  rejects a UTF-16 or UTF-32 file and Pylint reports `F0010` instead.
* `C0403` `invalid-characters-in-docstring` runs only with the `enchant`
  spelling dictionary, which is not available.

Known divergences from Pylint, where Pylint infers a type or a value:

* `C0209` knows a dict or list of more than one item, and a starred list,
  only when it is written in the call or held by a name assigned once to
  one; Pylint infers any value. A call that Pylint refuses for a backslash
  in the rendered argument is judged from the literals inside the argument.
* `W1307` checks only an argument written as a number, a boolean, `None` or
  `...`, and only the first lookup of a field: no other type is known not to
  be subscriptable.
* `E0119` fires when `print` is the builtin, which is the only case Pylint
  cannot infer.
* `E1200` and `E1201` take as a logger the logging module itself, a call of
  its `getLogger`, and a name or attribute (`self.logger`) assigned from
  `getLogger` or `Logger` anywhere in the file. Pylint finds a logger by its
  class, so a logger that comes from another call or a parameter is missed.
* `W1300` prints a float key as Rust does, so an exponent differs from
  Python's (`1e+100`).
* `E2511` keeps Pylint's line number, which counts only `\n`, but a lone
  `\r` still ends a line for the other native rules.
* `C0327` and `C0328` follow the logical lines of the ruff tokenizer, which
  match Python's `NEWLINE` tokens on the files tried; a file that does not
  end in a newline counts as ending in CRLF for `C0328`, as in Pylint.

## Batch B: classes, slots, special methods and calls

* `E0211` `no-method-argument`, `C0203` `bad-mcs-method-argument` and
  `C0204` `bad-mcs-classmethod-argument`; the last two read
  `valid-classmethod-first-arg` (default `cls`) and
  `valid-metaclass-classmethod-first-arg` (default `mcs`).
* `W0213` `implicit-flag-alias`: integers assigned under a class that
  derives from `enum.IntFlag`, with Pylint's wording and its quirks (any
  `name = <int>` under the class counts, nested scopes included).
* `R0904` `too-many-public-methods`; it reads `max-public-methods`
  (default 20).
* `E0236` `invalid-slots-object`, `E0238` `invalid-slots`, `E0242`
  `class-variable-slots-conflict` and `E0245` `declare-non-slot`.
* `E0301` `non-iterator-returned`, `E0306` `invalid-repr-returned`, `E0310`
  `invalid-length-hint-returned`, `E0311` `invalid-format-returned`, `E0312`
  `invalid-getnewargs-returned` and `E0313` `invalid-getnewargs-ex-returned`.
* `E3701` `invalid-field-call`, `W1115` `non-str-assignment-to-dunder-name`,
  `W1506` `bad-thread-instantiation`, `W1503` `redundant-unittest-assert`,
  `W0715` `raising-format-tuple`, `W1114` `arguments-out-of-order` and
  `W1117` `kwarg-superseded-by-positional-arg`.

Ruff has `PLR0904` for `R0904` only as a preview rule (in 0.17.1), so it is
ported. The other rules have no Ruff counterpart.

Known divergences from Pylint, where Pylint infers a value or a class:

* `C0203` and `C0204` take a class as a metaclass when it is `type`, an
  `abc` or `enum` metaclass, or a class of the file derived from one. A
  metaclass imported from another module (Django's `ModelBase`, say) is not
  known.
* `W0213` knows `IntFlag` by an import of `enum.IntFlag` or a class of the
  file derived from it. A class derived from an `IntFlag` of another module
  is not known. A bool value counts as the integer it is (`True` is 1).
* `R0904` follows `pylint_django` when `load-plugins` names it: a class that
  has a `TestCase` base and is named `...Test` or nested in a class, and a
  `...Admin` class with a `ModelAdmin` base, are not counted. An imported
  base is a `TestCase` when its name ends in `TestCase`, other than the
  simple, transaction, live-server and `unittest` ones, since its ancestry
  lives in another file.
* The slot rules read `__slots__` only as a list, tuple, set or dict written
  in the class body (its last assignment), with literal items: a name, a call
  or an operation is not followed. `E0236` shows only a constant item
  (`1`, `''`, `None`, `b'x'`). `E0245` needs every base to be a class of the
  file with such slots, where Pylint also infers imported bases and ignores
  one it cannot infer.
* The `*-returned` rules judge the one `return` of a method. The value is
  known only when it is a literal, a signed number, an f-string with no
  interpolation, `NotImplemented`, a display, a lambda, a generator
  method, a method with no `return` that is neither a stub nor abstract
  (`None`), or `self` of a class whose bases are all classes of the file
  (`__iter__` then needs a `__next__` in it or them). A name, a call, an
  operation, a conditional expression or a comprehension is unknown and
  reports nothing, where Pylint infers it. `abc.abstractmethod` is found
  through the import. `E0313` follows Pylint: an item of the returned tuple
  that is not a name or a call must be a tuple display (first) or a dict
  display (second).
* `W1114` and `W1117` resolve a call to a name bound once to an undecorated
  function of the file, to `self.method` of the enclosing class (undecorated
  or `staticmethod`), or to a class of the file with an `__init__` and no
  decorator, metaclass or `__new__`. A starred argument or `**` skips the
  call.
* `W1503` takes `self` as a `TestCase` when a base is named `...TestCase`
  (through imports and classes of the file), where Pylint finds the method on
  the inferred class. A float constant prints as Rust does, so an exponent
  differs from Python's.
* `E3701` and `W1506` follow imports of `dataclasses.field`,
  `dataclasses.make_dataclass` and `threading.Thread`, under any alias; a
  dataclass is a class decorated with a name `dataclass`, as in Pylint.
* `W1115` checks a constant, a display or a lambda on the right-hand side,
  not an operation (`1 + 1`) or a name that holds one.

## Batch C: control flow, statements and expressions

Functions and `try`:

* `W0101` `unreachable`: code after `return`, `raise`, `break`, `continue`
  or a call that never returns (see below); a bare `yield` that only marks a
  generator is not counted.
* `R1710` `inconsistent-return-statements` and `R1711` `useless-return`;
  like Pylint they leave `async def` alone.
* `R0911` `too-many-return-statements` (`max-returns`, default 6), `R0913`
  `too-many-arguments` (`max-args`, default 5, with
  `ignored-argument-names`) and `R0914` `too-many-locals` (`max-locals`,
  default 15). The first argument of a method and of a class method is not
  counted, and a name declared `global` is not a local.
* `W1113` `keyword-arg-before-vararg` and `E0701` `bad-except-order`.
* `W0135` `contextmanager-generator-missing-cleanup`.

Conditions and expressions:

* `W0125` `using-constant-test` and `W0126`
  `missing-parentheses-for-call-in-test`.
* `R1709` `simplify-boolean-expression`, `R1726` `simplifiable-condition`
  and `R1727` `condition-evals-to-constant`.
* `R1738` `chained-comparison-all-equal` and `R1739`
  `impossible-comparison`: the graph of `and`-ed comparisons of Pylint, with
  its cycle search.
* `W3601` `bad-chained-comparison` and `C0117` `unnecessary-negation`.

Statements and text:

* `W0105` `pointless-string-statement`, `W0124` `confusing-with-statement`,
  `C0200` `consider-using-enumerate` and `R1713` `consider-using-join`
  (it reads `suggest-join-with-non-empty-separator`).
* `W4701` `modified-iterating-list`, `E4702` `modified-iterating-dict` and
  `W4801` `looping-through-iterator`.
* `E0113` `invalid-star-assignment-target` and `E3102`
  `positional-only-arguments-expected`.
* `C0302` `too-many-lines` (`max-module-lines`, default 1000), reported on
  line 1 and counting lines like `wc -l`, and `C0325` `superfluous-parens`,
  which reads the tokens of the file as Pylint does.
* `R5102` `http-response-with-content-type-json`, a `pylint-django` rule
  that a Django project meets: a call whose callee ends in
  `HttpResponse` and has a `content_type` keyword whose text holds
  `application/json`; it reports only under the `pylint_django` gate of batch E.

`match` statements: `E1901` `bare-name-capture-pattern`, `E1902`
`invalid-match-args-definition`, `E1903` `too-many-positional-sub-patterns`,
`E1904` `multiple-class-sub-patterns`, `R1905` `match-class-bind-self` and
`R1906` `match-class-positional-attributes`.

Not ported: `R0917` `too-many-positional-arguments` (a project may disable it),
`E4703` `modified-iterating-set` and the other `pylint-django` rules
`R5101` and `R5103`, which a typical run does not need.

Known divergences from Pylint, where Pylint infers a value, a class or a
call:

* A call never returns, for `W0101` and `R1710`, when it is `exit`, `quit`,
  `sys.exit`, `os._exit`, `os.abort`, `pytest.fail` and its kin, an
  `assert_never`, `self.fail` of a class with a `...TestCase` base that does
  not define `fail`, a function of the file annotated `NoReturn` or `Never`,
  or a name of `never-returning-functions`. A function imported from
  another module that is annotated `NoReturn`, and a method called on a
  value that is not `self` (click's `ctx.exit()`), are not known. Only a
  call written as a statement ends a block for `W0101`.
* `R1710` follows a `raise` inside a `try` as Pylint does: the exception is
  the class named by the call, and a handler that names it must end in a
  return. A raised name that is not a class is assumed to be inferable,
  except a parameter, a name bound by unpacking, an undefined name and
  `queue.Empty`/`queue.Full` (which Pylint cannot infer, having two
  definitions). A name that holds `None` and an exception is not detected.
* `ignored-argument-names` is a regex in Pylint; here it can use literals,
  `.`, `.*`, `^`, `$` and `|`, as the default does. The kind of a method is
  read from `staticmethod` and `classmethod` only, not from other
  decorators.
* `E0701` knows the builtin exceptions and the classes of the file, followed
  through their bases of the file. An exception imported from another module
  is not known, nor are the classes it derives from.
* `W0125` and `W0126` know a test that is a name bound once to an
  undecorated function, a class, a lambda or a generator expression of the
  file, a module imported with `import`, or a builtin function or class; and
  the call of a function of the file that returns only generator
  expressions. A test such as `self.method` or a property, or a function
  imported with `from`, is not known.
* `R1709`, `R1726` and `R1727` take a value as constant when it is a
  literal, a container, a lambda, or `not`, `-` and `+` of one; Pylint also
  folds operations and comparisons of constants and names that hold them.
  The code a message quotes is the source text, except for strings, `not`,
  comparisons and boolean operations, which are written as astroid does.
* `C0117` fires for a comparison whose two sides have a known type: a
  literal (not `None`), `*args`, `**kwargs`, a name bound once to a literal,
  a call of a builtin class, or `len` of one of these. Pylint infers more.
* `W4701` and `E4702` take the iterated name as a list or dict when it is
  assigned a display, `list()`, `dict()` or `list` of a display of
  constants; an attribute (`self.items`) is not followed. `W4801` finds an
  iterator in a generator expression, a generator function of the file and
  the calls of `map`, `filter`, `zip`, `iter`, `reversed`, `enumerate` and
  `itertools` (through imports).
* `E3102` follows a name bound once to a function of the file, not a
  method. `W0135` follows a call by name or `self.method()` of the enclosing
  class to a generator function of the file.
* The class of a class pattern (`E1903`, `E1904`, `R1905`, `R1906`) is a
  builtin class, or a class of the file whose `__match_args__` is a tuple of
  strings written in its body or in a base of the file. A class from another
  module is not known.
* `W1113`, `R0913` and the other rules over a function body run on `def` and
  `async def` alike, except `R1710` and `R1711` as said above.

## Batch D: imports, variables and names

Imports:

* `C0415` `import-outside-toplevel`: an import in a function or class body;
  `allow-any-import-level` lists the modules that may be imported there.
* `W0404` `reimported` and `W0416` `shadowed-import`, with Pylint's rules
  for the frame (the function, then the module) and for imports in branches
  that exclude each other (`if`/`else`, `try` body and handlers).
* `W0407` `preferred-module`, from `preferred-modules`
  (`module:preferred`, comma separated), including Pylint's match of a key
  that is only a substring of the root module.
* `W0401` `wildcard-import`: a `from m import *`, reported at the import as
  `Wildcard import m` (the module as written, without the dots of a
  relative import). Like Pylint it skips an `__init__.py`, and it reports
  one in a function or class too. `allow-wildcard-with-all` excuses a
  module that binds `__all__` (also under `if` and `try`, by `=`, `+=` or an
  annotation); a module that is not found is never excused.
* `W0614` `unused-wildcard-import`: the public names of a `from m import *`
  module that the file never reads, reported at the import as
  `Unused import(s) a, b and c from wildcard import of m` (`m` as written,
  without the dots of a relative import). The names are
  those the module binds at its top level (imports, and names bound under
  `if`, `for`, `while`, `with`, `match` and `try`, including `except ... as`,
  `+=`, walruses and the names a function assigns after declaring them
  `global`), then those of its `from` imports and of the modules it
  star-imports, in astroid's order; names that start with an underscore
  and those of `from __future__` are left out. The first statement
  that binds a name lists it: a later star leaves out a name an earlier
  star or `from m import name` already bound. Like Pylint it
  ignores the `__all__` of the imported module, skips an `__init__.py`
  and a star in a function or class, and counts the names in the file's own
  literal `__all__` as read. Divergences: a module is found as base
  classes are (the directories above the file, and the standard library
  and packages of `VIRTUAL_ENV`), so a module that is not found, or is a C
  module, is skipped (a module is also matched by the case it is spelled
  in, even where the file system ignores it); the result depends on another
  file, so it is as stale as the cache is (see the project module's shortcut
  note).

Variables and scopes:

* `W0601` `global-variable-undefined`.
* `W0621` `redefined-outer-name`: a name of a function (a parameter
  included) that the module also binds, and an `except ... as name` inside
  another one with that name. `dummy-variables-rgx` and
  `ignored-argument-names` excuse names, read as real regular expressions.
* `W0631` `undefined-loop-variable`.
* `W0641` `possibly-unused-variable`: the unused names of a function that
  calls `locals()` on a later line.
* `E0606` `possibly-used-before-assignment`.

Names and calls:

* `C0104` `disallowed-name`; it reads `bad-names`, `bad-names-rgxs`,
  `good-names` and `good-names-rgxs`.
* `W3101` `missing-timeout`; it reads `timeout-methods`.

Not ported:

* `W0602`, `W0604` and `W0642` are Ruff's `PLW0602`, `PLW0604` and
  `PLW0642`; `W0613` `unused-argument` is Ruff's `ARG` family and needs the
  ancestors of a method to excuse an override. The `global` logic of `W0602`
  is still here, to tell `W0601` from it. Pylint skips `W0603` for a
  statement that gets `W0601` or `W0602`; the `global-statement` plugin
  reports it anyway.
* `W0237` `arguments-renamed` and `E0203` `access-member-before-definition`
  need the class hierarchy across files.
* `E0601` `used-before-assignment` shares the filter of `E0606` but gets its
  other cases from `try` blocks, which are not run (see below).

Known divergences from Pylint, where Pylint infers a value or a call:

* `C0415` and `W0407` read the names as written, like Pylint.
* `W0404` and `W0416` look for the first matching import in the body of the
  frame, as `_get_first_import` does, and stop at it even when it is
  exclusive with the statement. `W0404` also reports a name that one `from`
  statement imports twice, with Pylint's wording.
* `W0621` finds the names of a function's scope from the syntax, like
  astroid's locals, so an assignment under `nonlocal` counts as a local. The
  names that a `from module import *` binds are not known, since the module
  is not read; Pylint knows them and reports a function that redefines one.
* `W0631` reports a read after a loop only when the loop variable is the one
  binding of the name read (a binding inside the loop does not count, nor
  one in another branch of an `if` or `try`), and the loop may not run:
  Pylint infers the iterable. Here the iterable is safe when it is `range`,
  `enumerate` of a safe one, a non-empty display, a conditional expression
  whose first branch is safe, or a name whose first binding is any of these.
  A loop over a call or an attribute is judged not safe, as Pylint does when
  it cannot infer.
* `W0641` counts a name as read when any read of it resolves to the
  function, in any nested scope; `@overload` stubs and abstract methods are
  not excused.
* `E0606` runs the `if` filter of Pylint (`_uncertain_nodes_if_tests`),
  the `except`-name filter and the `del` rule, for the first read of a name
  in a module, function or class. An `if` test is a constant when it is a
  literal, `TYPE_CHECKING` or a name bound once to a literal; Pylint also
  infers the platform and version checks (`sys.platform == "linux"`), whose
  result then depends on the interpreter, and reports `E0601` instead.
  Pylint treats two tests alike when it cannot infer either of them, which
  it does for a call of a builtin function of unknown arguments
  (`isinstance`, `len`, `print`, ...); the list in `unassigned.rs` is what
  Pylint 4 does for each builtin function. A `break` anywhere in an `if`
  makes it permissive (Pylint passes `Continue` as the class to skip, so a
  `continue` does not).
* `C0104` checks a module-level name only when it gets a literal constant,
  which is all Pylint checks for the default bad names. An overridden method
  and an attribute that an ancestor defines are looked up among the classes
  of the file by name.
* `W3101` knows `requests.get` for `requests.api.get`, and no other
  re-export. A `**` argument is read as the empty dict for the `**kwargs`
  parameter of the enclosing function and as the literal when it is a dict
  display of strings; any other `**` skips the call, as Pylint does when
  it cannot infer the keywords.

## Batch E: the pylint_django messages

These ten rules, and `R5102` http-response-with-content-type-json, port the
checkers of the `pylint_django` plugin that the syntax decides. They report
only when the option `load-plugins` names `pylint_django` (a comma list), so
a project that does not use the plugin is not affected. A `.pylintrc`
project carries the option already; a `ferrolint.toml` project must set it:

```toml
[options]
load-plugins = "pylint_django"
```

* `E5101` model-unicode-not-callable, `W5102` model-has-unicode and
  `W5103` model-no-explicit-unicode, `W5104` modelform-uses-exclude.
* `E5141` hard-coded-auth-user and `E5142` imported-auth-user.
* `R5101` http-response-with-json-dumps, `R5102`
  http-response-with-content-type-json and `R5103`
  redundant-content-type-for-json-response.
* `W5150` format-html-interpolation and `W5151` mark-safe-interpolation.

Skipped: `W5101` model-missing-unicode never fires on Python 3, the plugin
returns before it; `E5110`, `F5110` and `F5101` need Django to import and
configure; `W5199` and the foreign-key and migration transforms are
internal to astroid or off by default.

Known divergences from Pylint:

* A class is a Django `Model` or `ModelForm` by the last name of a base
  (`Model`, `models.Model`, `ModelForm`, `forms.ModelForm`) or by a base
  class of the same file that is one. A base imported from another file
  that derives from a model, and a custom base named otherwise, is not
  seen. `W5103` looks for the inherited `__unicode__` (or a `__str__` under
  `python_2_unicode_compatible`) in the classes of the same file only.
* `E5101` reports a `__unicode__` assigned a literal (string, number,
  `None`, tuple, list, dict, set, f-string); a name or call is taken as
  callable, where Pylint infers it.
* The callee of `HttpResponse`, `JsonResponse`, `format_html`,
  `format_html_join` and `mark_safe` is matched on its source text, as
  Pylint matches its `as_string()`, so differences in spacing inside a
  dotted callee are not normalized.

## Batch F: naming and exceptions

Names (`C0103`, `C0105`, `C0131`, `C0132`):

* `C0103` `invalid-name`: Pylint's `NameChecker` with the five naming
  styles and every `*-naming-style` and `*-rgx` option of the kinds module,
  const, class, function, method, attr, argument, variable,
  class-attribute, class-const, inlinevar, typevar, paramspec,
  typevartuple and typealias. It reads `good-names`, `good-names-rgxs`,
  `bad-names` (a name they settle is left to `C0104`), `bad-names-rgxs`,
  `include-naming-hint` and `property-classes`, and prints the hint as
  Pylint does. A module-level constant is a name assigned a constant, as
  Pylint decides it: reassigned, in a loop or bound by a function it is a
  variable; in an `if __name__ == "__main__":` block either style passes.
  Properties and setters are attributes, a method that overrides an
  ancestor's is not judged, `Final` and enum members are class constants,
  a dataclass field is an instance attribute, and a PEP 695 type
  parameter or `type` alias is checked as its kind.
* `C0105` `typevar-name-incorrect-variance`, `C0131`
  `typevar-double-variance` and `C0132` `typevar-name-mismatch`.
* `C0102` `blacklisted-name` is the old name of `C0104`; Pylint 4 reports
  `C0104`, so there is no separate rule.

Exceptions (`W0718` and the checks Ruff lacks):

* `W0718` `broad-exception-caught` and `W0719` `broad-exception-raised`;
  both read `overgeneral-exceptions` (default `builtins.BaseException`,
  `builtins.Exception`) by the qualified name of the class, also behind an
  alias (`Alias = Exception`, `from builtins import Exception as E`). A
  handler with a `raise` among its own statements is exempt.
* `E0702` `raising-bad-type`, `E0705` `bad-exception-cause`, `E0710`
  `raising-non-exception`, `E0712` `catching-non-exception` and `W0716`
  `wrong-exception-operation`.

Not ported, since Ruff has them: `E0704` (`PLE0704`), `E0711` (`F901`),
`W0705` (`B014`, `B025`), `W0706` (`TRY203`), `W0707` (`B904`) and `W0711`
(`PLW0711`). `E0701` is in batch C, `W0702` is a plugin and `W0715` is in
batch B.

Known divergences from Pylint, where Pylint infers a value or a class:

* A module-level `name = value` is a constant when the value is a literal,
  a folded literal, an f-string, `int`, `str`, `bool` or `len` of literals,
  a name bound once to one, or a boolean, comparison or `if` expression of
  them. Pylint infers more (`os.name`, `"a".upper()`, `os.sep + "x"`,
  `isinstance(1, int)`, a constant attribute of an imported module or
  class); those names are not judged here. A value that cannot be told is
  skipped, as Pylint skips what it cannot infer. A call of a capitalized
  imported name is taken as an instance, a lower-case imported name as a
  function, and a call of a function of the file that returns only classes
  as a class.
* An ancestor's methods and attributes are looked up in the classes of the
  file and in the other modules of the project, which are found as files
  under the directories above the linted file and followed through
  `import` and `from` statements at the top level (see `native/project.rs`).
  Classes from installed packages are not read: of those only the test
  cases are known, a class named `...TestCase` standing for
  `unittest.TestCase`, and for Django's `TestCase` (which owns
  `setUpTestData`) unless its name holds `Transaction`, `Simple` or
  `LiveServer`. Because the answer depends on other files, a cached result
  is stale until the file changes or the cache is cleared with
  `--no-cache`.
* `name-group` is not read, and a `*-rgx` pattern that Rust's regex engine
  rejects (a look-around or a back-reference) is not applied.
* `E0710`, `E0712` and `E0705` judge a class only when all its bases are
  known (classes of the file or builtins). `E0702` takes the type of a
  literal, a signed number, a function of the file or a lambda; a name
  bound to a constant is not followed. `W0716` needs both operands to be
  classes of the file, builtins or tuples written out. `E0712` leaves a
  `|` of classes alone unless both are classes of the file or builtins.

## Batch G: class hierarchy

* `W0223` `abstract-method`: a class that does not override a method one
  of its ancestors declares abstract, by a body whose first statement
  (after the docstring) raises `NotImplementedError` or by
  `@abc.abstractmethod` or `@abc.abstractproperty`. As in Pylint, a class
  that is itself abstract (a protocol, an `abc.ABC` or `ABCMeta` class, or
  one with an abstract method of its own) is not reported, the ancestors
  are walked in C3 order, and a name the class binds in any way counts as
  an override.

Known divergences:

* The ancestors come from the classes of the file, the project's modules
  (see `native/project.rs`) and, when the `VIRTUAL_ENV` environment
  variable names a virtual environment, the standard library of its
  interpreter (from `pyvenv.cfg`) and its `site-packages`. A class with a
  base from anywhere else (a C module, an environment that is not active,
  a base computed at run time) is skipped, so its findings are missed
  rather than guessed. Without an active environment the base classes of
  the dependencies (a serializer's `create`, for instance) are unknown,
  and those findings go missing.
* A cached result does not track the environment: after switching it,
  lint with `--no-cache`.
* In an ancestor other than the class itself, a name bound by an import
  or a nested class removes an abstract method, where Pylint keeps it.

## Batch H: instance attributes and overrides

* `W0201` `attribute-defined-outside-init`: an instance attribute assigned
  only outside the methods of `defining-attr-methods`, a property setter
  and a `property(fget, fset)` setter. As in Pylint, a class attribute of
  the class or an ancestor, an attribute an ancestor assigns in a defining
  method, a `setattr(self, "name", ...)` there, and a method that a
  defining method calls as `x.method()` excuse it; `self.x += 1` alone and
  `__dict__` are left out, and `mixin-class-rgx` with
  `ignored-checks-for-mixins` skips mixins (and `E0203` with them).
* `E0202` `method-hidden`: a method whose name an instance attribute of
  the class or an ancestor, assigned in a method itself, takes; a property,
  a setter, a `functools.cached_property`, a decorator class with
  `__get__` and `__set__`, an ancestor's method of that name or a private
  name an ancestor assigns spares it.
* `E0203` `access-member-before-definition`: `self.x` read in the one
  function that assigns it, above that assignment, or a method called in
  `__init__` above it that reads it, unless a `try` that catches
  `AttributeError` or `Exception` keeps the two apart.
* `R0902` `too-many-instance-attributes`: more distinct instance
  attributes than `max-attributes` (default 7), the fields of a
  `@dataclass` included.
* `W0221` `arguments-differ` and `W0237` `arguments-renamed`: a method
  against the first ancestor in C3 order that binds its name to a
  function (the implementation, not an `@overload` stub). The positional
  parameters are compared by position and name (`dummy-variables-rgx`
  names aside), the keyword-only ones by name, and a removed `*args` or
  `**kwargs` is reported; a constructor is never compared and a special
  method only by number, as in Pylint. Private methods, setters and stubs
  are skipped.
* `W0231` `super-init-not-called`: an `__init__` that calls neither
  `super().__init__` (anywhere in its body) nor `Base.__init__` of a base
  whose `__init__`, found along the base's MRO, is not abstract (a `pass`,
  a docstring alone or `raise NotImplementedError`). A protocol base or a
  called `__init__` that another base shares ends the check.

Known divergences:

* Pylint infers what `x` in `x.name = value` stands for. Here it is the
  first parameter of a method of the class (or of a function or lambda
  nested in it that does not rebind it), or a name bound once in its
  scope to a call of a class of the file, `x = Model()`, of `cls` in a
  classmethod of the class, or of a name bound once to such a class or to
  a call of a function of the file that returns one. An instance reached
  any other way (`self.root.x = 1`, `cls.manager.model = ...`,
  `x = self.__class__()`, a name bound twice) gives the class nothing, so
  its `W0201` is missed.
* The ancestors are those of Batch G: a class whose MRO is unknown is not
  checked by `E0202`, `E0203`, `W0221`, `W0237` and `W0231`, nor by the
  ancestor tests of `W0201`. A builtin base such as `dict` or
  `Exception` adds nothing, which matches Pylint for the signatures it
  reads from a running interpreter.
* `E0202` treats a decorator that is a function as one that returns no
  data descriptor, where Pylint infers the value it returns.
* A method that a defining method calls excuses `W0201` when any call in
  that method has its name as an attribute (`x.prepare()`), where Pylint
  asks that the callee infer to that bound method.
* `__slots__` keeps astroid from recording an attribute outside them, for
  `R0902`, `W0201`, `E0202` and `E0203` alike, when the class and every
  ancestor of the file or project give plain slots (a string, list, tuple,
  set or dict of strings) and one of them names a slot. A builtin base
  other than `object` is not in the MRO here, where Pylint finds it without
  slots and records every attribute.

## Batch I: versions, `super()` and duplicate arguments

* `W2601` to `W2607`, the `using-*-in-unsupported-version` messages: syntax
  newer than the `py-version` setting. They report only when `py-version`
  is set, and only below the version that introduced the construct: an
  f-string below 3.6 (`W2601`), a `typing.final` decorator below 3.8
  (`W2602`), `except*` or `ExceptionGroup` below 3.11 (`W2603`), a PEP 695
  type alias or type parameter below 3.12 (`W2604`), a walrus below 3.8
  (`W2605`), a positional-only parameter below 3.8 (`W2606`) and a
  starred element in a list, set or generator comprehension below 3.15
  (`W2607`). The positions follow Pylint: the whole f-string, walrus,
  `try` or alias, the decorator expression, each type parameter, and the
  `def` line (or the lambda) for positional-only parameters.
* `E1003` `bad-super-call`: `super(X, self).m()` in a method where `X` is
  `type(self)`, `self.__class__`, or a class that is neither the class nor
  one of its ancestors; the calls of nested functions count, those of
  nested classes belong to their own methods.
* `E0108` `duplicate-argument-name`: a function parameter named twice.
  The parser accepts it, so the rule reports each repeat after the first,
  in signature order, on the repeated name.

Known divergences:

* The parser always reads the newest grammar, so a construct the
  `py-version` lacks never makes a file fail to parse. `ParamSpec` reports
  nothing under `W2604`, as in Pylint.
* `W2607` covers the starred element of a list, set or generator
  comprehension; the `{**x for ...}` dict form is not ported, since the
  syntax tree has no node for it.
* `E1003` skips a first argument that does not name a class of the file,
  the project or the active environment (see Batch G), where Pylint
  reports the name of an argument it cannot infer. A class whose MRO is
  unknown is skipped too.

## Batch J: Pylint configuration and pragmas

These rules read the names in a Pylint configuration file and in `# pylint:`
comments. A configuration finding sits on line 1 of the file, as Pylint
reports it, and only a Pylint file gets them (`.pylintrc`, `pylintrc`,
`pyproject.toml`, `setup.cfg`, `tox.ini`); `ferrolint.toml` already rejects
unknown keys. The names come from Pylint 4.1.3, its bundled extensions and
`pylint_django` 2.8.0, dumped by `make pylint-names` into
`src/pylint_names.rs`.

* `E0011` `unrecognized-inline-option`: a `# pylint: key=` comment whose key is
  not `disable`, `enable`, `disable-next`, `disable-msg` or `enable-msg`.
* `W0012` `unknown-option-value`: a `disable` or `enable` option, or a pragma,
  names a message, category or checker Pylint does not know. A name the rule
  registry resolves, such as a plugin's rule, is known.
* `R0022` `useless-option-value`: the same, for a message Pylint removed or
  moved to an extension.
* `E0013` `bad-plugin-value`: a `load-plugins` module is in no project
  directory above the file, in the standard library or in the
  `site-packages` of the virtualenv `VIRTUAL_ENV` names. Without
  `VIRTUAL_ENV` nothing is reported.
* `E0014` `bad-configuration-section`: never reported. Pylint 4.1.3 defines it
  but does not emit it: the keys of a `[tool.pylint.<table>]` flatten into
  options, so an unknown key in any table is an `E0015`.
* `E0015` `unrecognized-option`: one finding listing every option key Pylint
  does not know.

Known divergences:

* `no-self-use` and `R0201` count as known: Pylint reports `R0022` for them
  unless `pylint.extensions.no_self_use` is in `load-plugins`.
* A `load-plugins` entry other than `pylint_django` and `pylint.extensions.*`
  silences `E0015`, `W0012` and `R0022` for the file: its options and
  messages are not listed. An entry that `E0013` reports does not: Pylint
  cannot import it, so it adds none.
* The `R0022` text omits Pylint's link to the release notes.

Not ported: `fatal`, `astroid-error`, `parse-error`, `config-parse-error`,
`method-check-failed`, `django-not-configured`, `django-not-available`,
`django-not-available-placeholder`, `c-extension-no-member` and the
`wrong-spelling-*` family: they report a failure of Pylint's own machinery or
need what ferrolint does not run (astroid, a Django setup, a spelling
dictionary).
