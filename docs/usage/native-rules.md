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

Native rules are active by default like every other rule, except the messages
that Pylint ships disabled (`C1804`, `C1805`, `R6105` and the pragma messages
of Batch N). They select,
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
  one (`"%(a)s" % dict(a=1, b=2)` is one); Pylint infers any value. A call
  that Pylint refuses for a backslash in the rendered argument is judged
  from the literals inside the argument.
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
* `W1114` and `W1117` resolve a call to a name bound once to a function of the
  file, to `self.method` of the enclosing class, or to a class of the file with
  an `__init__` and no decorator, metaclass or `__new__`. Pylint reads a
  decorated function by the signature as written, so a decorator (`click`'s
  commands included) does not hide the callee; a `classmethod`, a property and
  its `setter` do. A starred argument or `**` skips the call.
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
  generator is not counted. A bare `raise` before a `;` (`except X:raise;y`)
  ends a block too.
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
* `W4701` `modified-iterating-list`, `E4702` `modified-iterating-dict`,
  `E4703` `modified-iterating-set` and `W4801` `looping-through-iterator`.
* `E0113` `invalid-star-assignment-target` and `E3102`
  `positional-only-arguments-expected`.
* `C0302` `too-many-lines` (`max-module-lines`, default 1000), reported on
  line 1 and counting lines like `wc -l`, and `C0325` `superfluous-parens`,
  which reads the tokens of the file as Pylint does.

`match` statements: `E1901` `bare-name-capture-pattern`, `E1902`
`invalid-match-args-definition`, `E1903` `too-many-positional-sub-patterns`,
`E1904` `multiple-class-sub-patterns`, `R1905` `match-class-bind-self` and
`R1906` `match-class-positional-attributes`.

Not ported: `R0917` `too-many-positional-arguments` (a project may disable it).

Known divergences from Pylint, where Pylint infers a value, a class or a
call:

* A call never returns, for `W0101` and `R1710`, when it is `exit`, `quit`,
  `sys.exit`, `os._exit`, `os.abort`, an `assert_never`, `self.fail` of a
  class with a `...TestCase` base that does not define `fail`, a function
  annotated `NoReturn` or `Never` (of the file, or imported from a module of
  the project or an installed package), a `self.method()` that the class or a
  base annotates so (a base from another module too, `Base[T]` too), a
  `self.name()` that a method sets to such a function (`self.name = f`, or a
  parameter whose default is `f`), or a name of `never-returning-functions`.
  Not known: the standard library, a function re-exported by the module it is
  imported from, a property that returns one, and a method called on a value
  that is not `self` (click's `ctx.exit()`, `outcome.unwrap()`). `pytest.fail`
  and its kin are not listed: with every pytest tried (7.4.4, 8.0.2, 8.4.2,
  9.1.1) Pylint cannot infer them, so they never end a block. A call anywhere
  in an assignment or an expression statement ends a block for `W0101`, not
  one in the header of an `if` or `for`. A file that Ruff's parser rejects
  (`raise;stmt` on one line) has no findings but the syntax error.
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
  file, a module imported with `import`, a class imported with `from` that
  the environment holds (`from logging import PercentStyle`), or a builtin
  function or class; and
  the call of a function of the file that returns only generator
  expressions. Not known, so not reported: a method read through `self`
  (Pylint reports `if self.method:` unless the method is a property), a
  name or attribute bound to a call such as `getattr(...)`, and an instance attribute
  bound to the call of a generator method. A class attribute bound to a
  constant is not reported by Pylint either.
* `R1709`, `R1726` and `R1727` take a value as constant when it is a
  literal, a container, a lambda, or `not`, `-` and `+` of one; Pylint also
  folds operations and comparisons of constants and names that hold them.
  The code a message quotes is written as astroid does.
* `C0117` fires for a comparison whose two sides have a known type: a
  literal (not `None`), `*args`, `**kwargs`, a name bound once to a literal or
  to such a call, a call of a builtin class, `len` of one of these,
  `self.__dict__`, or
  `self.name` / `cls.name` when the enclosing class assigns `name` once, to
  a literal (a base class is not read). An instance of a class, a property
  or an attribute set from a parameter is not known, though Pylint infers it.
* `W4701`, `E4702` and `E4703` take the iterated name as a list, dict or set
  when it is assigned a display, `list()`, `dict()`, `set()` or `list` or
  `set` of a display of constants; an attribute (`self.items`) is not
  followed. `W4801` finds an
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
  and a star in a function or class, and counts the names of the file's own
  first `__all__` as read: a list or tuple of strings, `+` of two of one
  kind, `*seq` inside one, `list(...)` and `tuple(...)` of one, a name
  assigned before (its last assignment, not rebound in a branch or by an
  import), and `m.__all__` of a module it reads (a `from m import __all__`
  too). A value astroid infers to no list or tuple lists no name, so every
  name is reported: `sorted(...)`, a set, a comprehension, `None`, a list
  plus a tuple, a name assigned only later. When the value is a call or a
  name it cannot read, a slice or a product, it reports nothing; a name that
  a function, class or comprehension
  binds itself (a variable, a parameter) and reads there is not a use of the
  wildcard's name. Divergences: a module is found as base
  classes are (the directories above the file, and the standard library
  and packages of `VIRTUAL_ENV`), so a module that is not found is skipped
  (a module is also matched by the case it is spelled in, even where the
  file system ignores it); a `m.py` with a compiled
  `m.*.so` or `m.pyd` beside it is skipped, as astroid takes the compiled file
  and finds it empty; the result depends on another file, so it is as
  stale as the cache is (see the project module's shortcut note). A C or
  builtin module of the standard library (`math`, `itertools`, `time`) has
  no file, so its public names, sorted as Python's `dir` lists them, come
  from a table that `make c-module-names` dumps into
  `src/c_module_names.rs` from one Python version (3.14); another version
  may bind other names, a module of another platform is missing, and
  `builtins` lacks the type names (`generator`, `NoneType`, ...) that
  astroid adds to it.

Variables and scopes:

* `W0601` `global-variable-undefined`.
* `W0621` `redefined-outer-name`: a name of a function (a parameter
  included) that the module also binds, and an `except ... as name` inside
  another one with that name, or that a star import of the module binds.
  `dummy-variables-rgx` and
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
  reads the same two cases from the statement's names and skips it too.
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
  names that a `from module import *` binds are the public names of the
  module when it is found (the same list as `W0614`), and the message names
  the line of the first star import or definition of the name; a module that
  is not found binds none.
* `W0631` reports a read after a loop only when the loop variable is the one
  binding of the name read (a binding inside the loop does not count, nor
  one in another branch of an `if` or `try`), and the loop may not run:
  Pylint infers the iterable. Here the iterable is safe when it is `range`,
  `enumerate` of a safe one, a non-empty display, a conditional expression
  whose first branch is safe, or a name whose first binding is any of these.
  `range` may come from `from six.moves import range`, as in Pylint's six
  brain. A loop over a call or an attribute is judged not safe, as Pylint does
  when it cannot infer. Pylint infers `self.items` from the first
  `self.items = [...]` of the class, as this port does for a non-empty display
  (not for an attribute an ancestor assigns, or a `cls.items`); it also infers
  `self.items + [x]`, which is judged not safe here.
* `W0641` counts a name as read when any read of it resolves to the
  function, in any nested scope; `@overload` stubs and abstract methods are
  not excused.
* `E0606` runs the `if` filter of Pylint (`_uncertain_nodes_if_tests`),
  the `except`-name filter and the `del` rule, for the first read of a name
  in a module, function or class. An `if` test is a constant when it is a
  literal, `TYPE_CHECKING` or a name bound once to a literal, or a
  comparison of `sys.platform` with a string, which is judged against the
  platform ferrolint runs on, as Pylint judges it against its own: the two
  agree only when both run on the same system. Pylint also infers version
  checks (`sys.version_info >= (3, 9)`), which are not judged here.
  Pylint treats two tests alike when it cannot infer either of them, which
  it does for a call of a builtin function of unknown arguments
  (`isinstance`, `len`, `print`, ...); the list in `unassigned.rs` is what
  Pylint 4 does for each builtin function. A `break` anywhere in an `if`
  makes it permissive (Pylint passes `Continue` as the class to skip, so a
  `continue` does not). `hasattr(self, "x")` is true here when a method of the
  class assigns `self.x`, as astroid infers it (the negated test then leaves
  only the `else`, and Pylint reports `E0601`), and `hasattr(sys, "x")` is
  true when the module is a C module the host has a table for and the table
  holds `x`; an attribute that only an ancestor defines, or that nothing
  defines, leaves the test unknown. A name that a star import binds
  (`from _ast import *`) is not reported at module level, whatever the
  module.
* `C0104` checks a module-level name only when it gets a literal constant,
  which is all Pylint checks for the default bad names. An overridden method
  and an attribute that an ancestor defines are looked up among the classes
  of the file by name.
* `W3101` knows `requests.get` for `requests.api.get`, and no other
  re-export. A function of the linted file is listed under its module's
  name (`requests.api.request` for the `request` that `requests/api.py`
  defines). A `**` argument is read as the empty dict for the `**kwargs`
  parameter of the enclosing function and as the literal when it is a dict
  display of strings; any other `**` skips the call, as Pylint does when
  it cannot infer the keywords.

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
  A base written `Base[int]` is `Base`. A variable that holds a class is
  not judged: a class of the file or the project, `Outer.Inner`, what a
  function of the project defines and returns, and `type(x)` unless `x` is
  a call ferrolint cannot infer.
* `C0105` `typevar-name-incorrect-variance`, `C0131`
  `typevar-double-variance` and `C0132` `typevar-name-mismatch`.
* `C0102` `blacklisted-name` is the old name of `C0104`; Pylint 4 reports
  `C0104`, so there is no separate rule.

Exceptions (`W0718` and the checks Ruff lacks):

* `W0718` `broad-exception-caught` and `W0719` `broad-exception-raised`;
  both read `overgeneral-exceptions` (default `builtins.BaseException`,
  `builtins.Exception`) by the qualified name of the class, also behind an
  alias (`Alias = Exception`, `from builtins import Exception as E`), also
  one set in a top-level `if` or `try` (`except NameError: M = Exception`).
  A handler with a `raise` among its own statements is exempt.
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
  them, and so is a `str` or `bytes` method call on a constant (`"a".upper()`,
  `",".join(x)`, `"{}".format(1)`, which Pylint infers to a constant whatever
  the arguments, except `format`, which needs every argument constant and
  every field of the template supplied), a fixed list of `sys` and `os`
  constants (`os.sep`, `sys.maxsize`, `sys.platform`; also through
  `from sys import maxsize`) and `hasattr(mod, "name")` of an imported module
  the project finds. Pylint infers more (`os.name`, `os.sep + "x"`,
  `isinstance(1, int)`, a constant attribute of another imported module or
  class); those names are not judged here. A value that cannot be told is
  skipped, as Pylint skips what it cannot infer. A call of a capitalized
  imported name is taken as an instance, a lower-case imported name as a
  function, a call of a function of the file that returns only classes as a
  class, and a call of one without decorator, `async` or `yield` that has a
  single `return` as the value of it (a literal, a display, a nested function
  or a variable bound once). `str(x)` is a constant whatever `x` is, and
  `int(x)`, `float(x)` or `bytes(x)` of a value that is not constant is an
  instance: inferable, so a name bound to it twice is a variable, and a lone
  one is judged by its style (the constant style unless it is snake_case).
  An `if` expression of two constants of different
  types is not judged, as astroid cannot join them; one whose test astroid
  infers (`True`, `sys.platform`) picks an arm there and is not judged here
  either.
* An ancestor's methods and attributes are looked up in the classes of the
  file and in the other modules of the project, which are found as files
  under the directories above the linted file and followed through
  `import` and `from` statements at the top level (see `native/project.rs`).
  `C0103` also reads the standard library and the installed packages when
  `VIRTUAL_ENV` names an environment, so a base from either is followed to
  the class; without one only `unittest`'s test cases are known, by the names
  of their members. The builtin and standard library classes in
  `src/class_methods.rs` (the exceptions, `dict`, `logging.Handler`,
  `threading.Thread`, `json.JSONEncoder`, `argparse.Action`, ...) are known
  with every method of theirs and of their ancestors, from a table that
  `tools/class-methods.py` dumps from Python 3.14 through astroid, whatever
  the interpreter of the linted project is (Python 3.15 differs in a few
  private helpers, `str.maketrans`, `logging.Filter.__repr__` and the
  `pathlib` `__fspath__` and `is_reserved`); a class
  missing from it counts as having no methods. Because the answer depends
  on other files, a cached result is stale until the file changes or the
  cache is cleared with `--no-cache`.
* Left to the inference Pylint runs: a name bound to a class of the
  project is a class (and not judged) when the project binds it once, as
  astroid infers one value, also through `name = module.Class` in a package,
  a class defined in an `except` handler, a function that returns a class of
  the project, a metaclass called with a name, bases and a namespace and the
  functional `Enum("Name", ...)`. A base bound in several branches
  (`X = int` under `if`, `X = long` under `else`; an import under `try` and
  another under `except`) stands for each value, also in another module of
  the project, as Pylint takes every value of a base. A class value bound
  twice (a fallback in `try`/`except` or `if`), a class
  returned by a call of an installed package, a standard library class
  missing from the class table (`_Fraction = Fraction`), the value of a
  standard library attribute outside that list (`math.pi`, `socket.AF_INET`),
  `%` formatting, a name
  read through `getattr`, a variable whose value is a factory or
  an attribute of an installed package (`Pool = app.Pool`) and a name that a
  function binds with `global` to a value that cannot be told are not
  followed, so the names Pylint infers differently stay as differences. A
  name that a function rebinds with `global` to a value that can be told is
  judged as a variable, as Pylint does. An attribute that
  `obj = Cls(); obj.attr = 1` adds to `Cls` from outside its body is judged
  as one of the class (after the ones its methods set, and not for a class
  with `__slots__`).
  A comprehension is not inferred by astroid and its name is not judged.
* Pylint reports some attribute lines only when the file is linted alone, not
  inside a whole run (`fontTools/ttLib/tables/*`, which set an attribute of a
  base class from another module); ferrolint reports them in both cases.
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
* A base is followed through the imports of its module (`import m`, `from m
  import X`, also `from m import *` and a `try: import m` whose `except`
  assigns `m = None`) and through a top-level `Alias = Class` of the module
  that defines it. `typing.X[...]` is an empty class, as in astroid. A base
  that a function returns, a class bound under `if` or `try` by a plain
  assignment, a C class (`types.ModuleType`, `io.IOBase` through `_io`; but
  `ctypes.Structure` and `ctypes.Union` add no method) and a dataclass's
  generated `__init__` are not read.
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
  `__get__` and `__set__` (a subclass of `property` too), a decorator
  imported from a module that is nowhere (Pylint cannot infer it), an
  ancestor's method of that name or a private name an ancestor assigns
  spares it. The module in the message is named from the run's root, as
  Pylint names it, so a directory with no `__init__.py` above it is part of
  the name.
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
  are skipped. A base from `typing` that aliases a `collections.abc` class
  (`typing.MutableMapping[str, str]`) is that class; `typing.IO[str]` and
  the other `typing` classes are empty, as astroid infers them.
* `W0231` `super-init-not-called`: an `__init__` that calls neither
  `super().__init__` (anywhere in its body) nor `Base.__init__` of a base
  whose `__init__`, found along the base's MRO, is not abstract (a `pass`,
  a docstring alone or `raise NotImplementedError`). A protocol base or a
  called `__init__` that another base shares ends the check. A dataclass with
  a field of its own and no `init=False` has the `__init__` astroid builds for
  it, so a subclass that skips it is reported.

Known divergences:

* Pylint infers what `x` in `x.name = value` stands for. Here it is the
  first parameter of a method of the class (or of a function or lambda
  nested in it that does not rebind it), or a name whose nearest binding in
  its scope is that parameter (`me = self`), a call of a class of the file
  (`x = Model()`, also annotated), of `cls` in a classmethod, of
  `self.__class__`, of a class the method's class binds in its body
  (`self._Result()`), of `C.__new__(...)` (a class without a `__new__` of its
  own) or of `object.__new__(C)`, or a name bound to such a class
  or to a call of a function of the file that returns one. An instance
  reached any other way gives the class nothing, so its `W0201` is missed
  and its `R0902` count is short: through an attribute or a property
  (`self.root.x = 1`, `self.fixup.worker = w`, `cls.manager.model = ...`),
  through a parameter, a list element, a loop variable or another module,
  through `type(self)`, from a name rebound in a branch (the nearest binding
  wins here, where Pylint follows every one that can reach the line) or from
  a class whose own `__new__` makes the instance.
* The ancestors are those of Batch G: a class whose MRO is unknown is not
  checked by `E0202`, `E0203`, `W0221`, `W0237` and `W0231`, nor by the
  ancestor tests of `W0201`. A builtin base such as `dict` or
  `Exception` adds nothing, which matches Pylint for the signatures it
  reads from a running interpreter.
* The signature rules miss an override whose ancestor astroid reaches by a
  path the project does not follow: a class of a C module or a builtin
  (`str.replace`), a name a module binds in a function with `global`
  (`concurrent.futures.ThreadPoolExecutor`), a base imported in both
  branches of an `if` or `try`, a base bound by a star import and then by an
  alias of the same name, and `Generic.__class_getitem__`, which astroid adds
  to `typing.Generic`. A name rebound to another class in a branch
  (`HTTPSConnection = DummyConnection` under `if`) makes ferrolint report
  an override that Pylint does not.
* `E0202` treats a decorator that is a function as one that returns no
  data descriptor, where Pylint infers the value it returns.
* A method that a defining method calls excuses `W0201` when that method
  calls it on its first parameter or on a local (`self.prepare()`,
  `inst.prepare()`; `prepare = _prepare` in the class names `_prepare`), where
  Pylint asks that the callee infer to that bound method. A call on a
  `self.helper` or on another parameter does not count. `W0201` skips what
  every class has from `object` (`self.__class__ = C`) and, for a class
  that derives from a builtin exception through bases of the file, what every
  builtin exception has (`args`, `__cause__`, `with_traceback`, ...); the
  attributes of one exception (`errno`, `msg`) are not known here. `E0203`
  skips the `object` attributes too.
* A base that astroid certainly fails to infer is left out of the ancestors,
  as Pylint leaves it out (for every rule that reads them): a name that only
  imports of a module the environment lacks bind (`from exceptiongroup import
  X` with `exceptiongroup` not installed), or a `Base[...]` whose value or
  single index is such a name (`AbstractRaises[BaseExceptionGroup[T]]`). It
  needs `VIRTUAL_ENV`; without it the class keeps an unknown MRO. A name that
  something else also binds (`except ImportError: X = object`) stays unknown.
  A name bound once to `make()`, a plain function of the file whose every
  `return` names one class, is that class (a mixin factory).
  A name that nothing binds and no star import can bring (every star reads a
  missing module) is left out the same way, and one a star module binds to an
  empty class (`Structure = ctypes.Structure`) adds nothing.
* `W0201` is missed where Pylint infers a base here unknown and the class has
  an unknown MRO: a base that is a call into another module
  (`getTableClass("glyf")` yields `DefaultTable` as well as what astroid cannot
  infer; `getFormatSwitchingBaseTableClass("uint8")` a class picked from a
  dict by its argument), a factory returning one of several classes, a factory
  of another module, or a name Pylint follows to a class.
* `__slots__` keeps astroid from recording an attribute outside them, for
  `R0902`, `W0201`, `E0202` and `E0203` alike, when the class and every
  ancestor of the file or project give plain slots (a string, list, tuple,
  set or dict of strings, written in the body or under `if` and `try`,
  where the assignments add up) and one of them names a slot. Slots built
  in a loop are not read. A builtin base
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
unknown keys. The names come from Pylint 4.1.3, and its bundled extensions,
dumped by `tools/pylint-names.py` into
`src/pylint_names.rs`.

* `E0011` `unrecognized-inline-option`: a `# pylint: key=` comment whose key is
  not `disable`, `enable`, `disable-next`, `disable-msg` or `enable-msg`.
* `W0012` `unknown-option-value`: a `disable` or `enable` option, or a pragma,
  names a message, category or checker Pylint does not know. A name the rule
  registry resolves, such as a plugin's rule, is known. A message or checker
  of a `pylint.extensions.*` plugin is known only when
  `load-plugins` names that plugin, as Pylint registers its names only then.
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

* A `load-plugins` entry other than `pylint.extensions.*`
  silences `E0015`, `W0012` and `R0022` for the file: its options and
  messages are not listed. An entry that `E0013` reports does not: Pylint
  cannot import it, so it adds none.
* The `R0022` text omits Pylint's link to the release notes.

Not ported: `fatal`, `astroid-error`, `parse-error`, `config-parse-error`,
`method-check-failed`, `c-extension-no-member` and the
`wrong-spelling-*` family: they report a failure of Pylint's own machinery or
need what ferrolint does not run (astroid, a spelling
dictionary).

## Batch K: messages of Pylint's bundled extensions

These five rules port checkers of `pylint.extensions`. They report only
when the option `load-plugins` names the extension (a comma list), because
Pylint loads none of them by default. A `.pylintrc` project carries the
option already; a `ferrolint.toml` project must set it:

```toml
[options]
load-plugins = "pylint.extensions.while_used, pylint.extensions.confusing_elif"
```

A plugin rule may take the msg-id or symbol of such a native rule (a plugin
that ports another `W9xxx` set does). While `load-plugins` leaves the
extension out, the native rule is not in the rule table and the plugin rule
answers to that msg-id in `select`, `ignore`, `per-file-ignores`, the
`disable` option and `# pylint:` pragmas, as in Pylint, where the two never
coexist. When `load-plugins` names the extension too, the run stops at
configuration load and says which extension holds the msg-id. This holds for
every rule of Batches K, M and W, `W0714` and `W3201` (the summary of each
says which module it needs).

* `W0149` `while-used` (`pylint.extensions.while_used`): every `while` loop,
  on its `while` line.
* `C3401` `dict-init-mutate` (`pylint.extensions.dict_init_mutate`): a
  statement `d = {...}` that a subscript assignment `d[k] = v` follows at once
  in the same block. The message suggests the literal with the keys of the
  assignments that follow in a row, cut after 64 characters. A tuple target,
  a chained target, an annotated target and a dict comprehension do not match.
* `R3501` `consider-refactoring-into-while-condition`
  (`pylint.extensions.consider_refactoring_into_while_condition`): a loop
  whose condition is a truthy constant and whose first statements are `if`
  statements that start with `break`, with their `elif` branches. The message
  gives the inverted conditions to move into the `while`.
* `R5601` `confusing-consecutive-elif` (`pylint.extensions.confusing_elif`):
  an `elif` right after an `if` block whose last statement is an `if` with no
  `else`, reported on the `elif` line.
* `R6106` `consider-math-not-float` (`pylint.extensions.code_style`):
  `float('inf')`, `float('-inf')` and `float('nan')` on the builtin, including
  a misspelling such as `float('infinty')`, which reads as the closest of
  `inf` and `nan` as Pylint's `difflib` match does. A string with only digits,
  signs, dots and `e` is a number and stays silent.

Known divergences:

* The message of `C3401`, `R3501` and `R6106` quotes expressions the way
  astroid prints them. A lambda, an f-string and a complex number with an
  exponent print as their source text or in Rust's form instead.
* `R6106` sees the callee `float` and `builtins.float`, including a name
  imported from `builtins`. Pylint also infers a call through an alias such as
  `f = float` and `f('inf')`; that one is not reported.
* The `code_style` extension's other messages are not ported here.

## Batch L: literal comparisons and slices

* `C1803` `use-implicit-booleaness-not-comparison`: `==`, `!=`, `<`, `<=`, `>`
  or `>=` between a value and an empty list, tuple or dict literal, with the
  value on either side. The message suggests `not x`, or for `!=` `x` in a
  truth test (`if`, `while`, `assert`, a conditional expression, `not`, a
  comprehension filter, `bool(...)`, `any`/`all` of a generator, `filter` of a
  lambda, and `and`/`or` inside them) and `bool(x)` elsewhere.
* `C1804` `use-implicit-booleaness-not-comparison-to-string` and `C1805`
  `use-implicit-booleaness-not-comparison-to-zero`: `==`, `!=`, `is` or
  `is not` against `""` or against `0`, `0.0` or `0j` (not `False`). Pylint
  ships both disabled, so ferrolint reports them only when `select` names
  them or an `enable` option reaches them (`enable=C1804`, `enable=all`).
* `E1143` `unhashable-member`: a list, dict or set as a dict key, a set
  member, or the subscript of a dict literal.
* `E1144` `invalid-slice-step`: `x[::0]` on a list, tuple, string or bytes;
  assigning or deleting a slice is checked for a list only, as Pylint does.
  A step of `False`, `0.0` or `0j` counts, `-0` does not.

Known divergences:

* Pylint infers the compared or subscripted value. Here a value has a type
  only when it is a list, tuple, dict, set, string or bytes literal, a call
  of the builtin `list`, `tuple`, `dict`, `set` or `str`, or a name whose every
  binding is a plain or annotated assignment, or an element of a tuple or list
  unpacking, to one of those, all of the same type. A parameter, an attribute,
  a call of another function, an instance of a class, a name bound to
  different types, `range(...)` and `slice(...)` objects are silent where
  Pylint may report.
* `C1803` is silent for a number, a float and a bool, as in Pylint, but
  Pylint also reports a `None`, a class, a function, a module and an instance
  of a class without `__bool__`; those are not reported.
* A category toggle (`enable=C`) does not reach a native rule, so it does not
  turn `C1804` or `C1805` on; name them or use `all`.
* `E1143` checks only the key or member itself: a comprehension, a
  conditional expression and an instance of a class are not looked at.
* `R6104` `consider-using-augmented-assign`, the `code_style` extension's
  other message, is not ported; Pylint disables it by default.

## Batch M: the code_style and bad_builtin messages

Like Batch K, these report only when `load-plugins` names the extension's
module.

* `R6101` `consider-using-namedtuple-or-dataclass`
  (`pylint.extensions.code_style`): a dict of more than one entry, written as
  the value of a module-level assignment or of an annotated assignment marked
  `Final` (`Final`, `typing.Final`, `Final[...]`), whose values are all dicts
  with string keys and one key in common, or all lists or tuples of one
  non-zero length that are not all made of dicts. Reported on the dict.
* `R6102` `consider-using-tuple`: a list literal as the iterable of a `for`
  statement or of a comprehension clause. An `async for` statement is not
  reported, as in Pylint.
* `R6103` `consider-using-assignment-expr`: an `if` whose test is a name, a
  `not` name or a comparison with one operator on a name, right after a
  one-line `name = value` or `name: T = value`. The message holds the whole
  `if` line with the first occurrence of the name replaced, as Pylint does it.
  Nothing is reported when `py-version` is below 3.8, when the suggestion is
  longer than `max-line-length-suggestions` (`max-line-length` when it is 0,
  100 by default) with the column of the `if`, or when a comparison is
  followed by another `if` testing the same name (a `match` fits better). It
  reads the `if` that opens a chain, so an `elif` never reports.
* `R6105` `prefer-typing-namedtuple` (`pylint.extensions.code_style`): a call
  to `collections.namedtuple`, through `import collections`, an alias or a
  `from` import. Pylint disables it by default and so does ferrolint: it
  reports after `enable=prefer-typing-namedtuple` (or `enable=all`) in a
  Pylint configuration, or with `select`.
* `W0141` `bad-builtin` (`pylint.extensions.bad_builtin`): a call by name to
  a function listed in `bad-functions` (`map` and `filter` by default) that
  neither the function around the call nor the module binds. The message for
  `map` and `filter` adds the hint to use a list comprehension.

Known divergences:

* `R6101` takes a dict key as a string only when it is a string literal;
  Pylint also infers a name or an expression to a string, and then compares
  the keys by their printed text.
* `R6103` prints an f-string and a lambda in the suggestion as their source
  text (see the divergences of Batch K).
* `R6105` sees the callee through the imports of `collections` only; Pylint
  also infers a call through an alias such as `maker = namedtuple`.

## Batch N: the messages about `# pylint:` pragmas

Pylint tells about its own pragmas with informational messages that it leaves
off by default, and so does ferrolint: they report when `select` lists them or
a Pylint `enable` names them by id or symbol (`enable=locally-disabled`).

A pragma is read as Pylint reads it: the last `pylint:` of a comment counts,
up to the next `;` or `#`, so `# pylint: disable=a; enable=b` is only the
`disable`.

* `I0010` `bad-inline-option`: `disable`, `enable`, `disable-next`,
  `disable-msg` or `enable-msg` with no names, or with names but no `=`. The
  message quotes the keyword.
* `I0011` `locally-disabled`: one finding per message a `disable`,
  `disable-next` or `disable-msg` pragma turns off, on the pragma's line (the
  next one for `disable-next`), as `Locally disabling unused-import (W0611)`.
  A category letter (`disable=W`) or a checker name (`disable=imports`) gives
  one finding for each of its messages. A legacy id or symbol gives the
  current message.
* `I0013` `file-ignored`: `skip-file`, `disable-all` or `disable=all`. Pylint
  stops reading the pragmas of the file there, and so do these five rules.
  It reports nothing else for the file, and neither does ferrolint.
* `I0022` `deprecated-pragma`: `disable-msg`, `enable-msg`, `disable-all` or
  `disable=all`, with the pragma to use instead.
* `I0023` `use-symbolic-message-instead`: a message named by its id in a
  `disable`, `enable`, `disable-next`, `disable-msg` or `enable-msg` pragma.
  A skipped file gets none.

A `disable` pragma hides these five messages from its own line to the end of
the innermost block, and the module ends with its last statement: a pragma
after it hides nothing, and a pragma never hides a message above it. This is
not the block rule of the other messages.

Known divergences:

* Only Pylint's own messages are known: a name that belongs to an extension
  or to a plugin gives no `I0011` and no `I0023`.
* `disable-next=all` and `disable-msg=all` give no `I0011`; Pylint gives one
  for each message that is off by default.
* `enable=I` and the other category letters do not switch these messages on
  (they never reach a native rule); name them by id or symbol.
* A comment after a backslash continuation is reported on its own line, and
  the names of one pragma hide one another only through the block rule above.
* `I0020` `suppressed-message` and `I0021` `useless-suppression` are not
  ported: they need to know which pragma silenced each finding, and `I0021`
  would call a pragma useless when it names a message ferrolint does not
  implement.

## Batch O: Ruff preview rules

ferrolint does not run Ruff: it only reads Ruff's parser crates, and a user runs
the `ruff` binary beside it. Ruff's preview rules are therefore out of reach
of a ferrolint setting, and these five Pylint messages, which Ruff has only
as preview rules, are ported.

* `E4703` `modified-iterating-set`: a set that the `for` loop iterating it
  adds to, removes from, clears or pops (`add`, `clear`, `discard`, `pop`,
  `remove`), or whose loop target it deletes. Beside `W4701` and `E4702`,
  with the same rules for what the loop iterates (see Batch C).
* `R0202` `no-classmethod-decorator` and `R0203` `no-staticmethod-decorator`:
  `name = classmethod(method)` or `staticmethod(method)` in a class body,
  where `method` is a function of the class. As in Pylint the callee is
  matched by its name alone, the argument must be a plain name, and the
  first binding of that name in the class must be a `def`. The message sits on
  the first assignment target.
* `W0244` `redefined-slots-in-subclass`: a slot of a class that an ancestor
  with `__slots__` already has, reported on the `__slots__` value as
  `Redefined slots ['a', 'b'] in subclass`. The slots of an ancestor are
  those of its whole MRO, and none when one class there has no plain slots,
  as in astroid.
* `W3201` `bad-dunder-name` (`pylint.extensions.dunder`): a method (not an
  `async def`, which Pylint skips) whose name starts and ends with `_` and is
  not one of Python's special names, those of `py-version` and
  `good-dunder-names`. Reports only when `load-plugins` names the extension.

Known divergences:

* `W0244` reads `__slots__` as the slot rules do (a display of strings written
  in the class body), so a value built by a call (`tuple([...])`) or a name
  is not followed. The ancestors come from the MRO of Batch G. A class of the
  file with a builtin base other than `object` has no slots, as in Pylint,
  but a class of another module with one is not told apart, so its slots
  count.

## Batch P: deprecated modules, methods and classes

Pylint keeps tables of the deprecated names of the standard library, each
with the Python version that deprecated it. `make pylint-deprecated` dumps
them from Pylint 4.1.3 into `src/pylint_deprecated.rs`; run it again with the
Python of a virtualenv that has the Pylint to follow
(`make pylint-deprecated PYTHON=/path/to/python`).

Pylint applies the entries up to the version of the interpreter it runs on.
Here the target is the `py-version` setting, and `3.14` (the version of the
table's interpreter) when it is not set. `deprecated-modules` adds its modules
to the table, as in Pylint.

* `W4901` `deprecated-module`: `import m`, `from m import x` and
  `__import__("m")` of a module in the table or below it
  (`distutils.core`). A relative `from` import is made absolute first, as
  Pylint does.
* `W4902` `deprecated-method`: a call of a function or method in the table,
  through the imports that name it (`logging.warn(...)`, `from logging import
  warn`, `import logging as lg`), or a call of a function of the file or a
  builtin whose bare name is in the table (`assert_(x)` after `def assert_`).
  The message names the function as called.
* `W4904` `deprecated-class`: `from m import C` and `import m.C` of a class
  in the table, and a call `m.C(...)` where `m` is a plain name; Pylint
  compares the text of the owner here, so `m` need not be an import. A `from`
  import directly under `if sys.version_info ...` or `if six.PY2/PY3` is
  not reported, as in Pylint.

Known divergences:

* Pylint reports only a deprecated name that astroid infers on its
  interpreter, so `time.clock` or `asyncio.coroutine`, removed from Python
  3.14, give nothing there. The dump keeps the spellings that Python 3.14.8
  infers (`asyncio.sleep` for `asyncio.tasks.sleep`, plus the names a public
  module re-exports) and drops the others, so
  those names stay silent here too, whatever the `py-version`; the version
  only filters the entries by their date.
* A callee that is a local variable, an instance or a call result is not
  followed: `logger.warn(...)`, `thread.isAlive()` and
  `logging.getLogger().warn()` give nothing, where Pylint reports them (twice
  for a logger, which it infers as two classes). `self.assert_(x)` is
  silent for the same reason.
* `__import__` is checked for a string literal only; Pylint infers a name to
  its value.

## Batch Q: deprecated arguments, decorators and attributes

The same tables and version target as Batch P (`make pylint-deprecated`).

* `W4903` `deprecated-argument`: a call of a function in the table that
  passes a deprecated argument by keyword (`asyncio.sleep(1, loop=None)`) or
  by position (`shutil.rmtree(p, False, handler)`). A call that `W4902`
  already reports is not checked again, as in Pylint.
* `W4905` `deprecated-decorator`: the first decorator of a function or class
  when it, or the function it calls, is in the table
  (`@abc.abstractproperty`, `@abc.abstractproperty()`). Later decorators are
  not looked at, as in Pylint.
* `W4906` `deprecated-attribute`: a read of an attribute in the table
  (`sys.last_type`, `calendar.January`). A store or a `del` is not a read.

Known divergences:

* The owner of a call or an attribute is followed through imports only. A
  name bound to a module, an instance or a call result
  (`alias = sys; alias.last_type`) is not, where Pylint infers it.

## Batch R: the arity of a call

* `E1120` `no-value-for-parameter`, `E1121` `too-many-function-args`, `E1123`
  `unexpected-keyword-arg`, `E1124` `redundant-keyword-arg` and `E1125`
  `missing-kwoa`: Pylint's match of the arguments of a call with the
  parameters of the function, with its messages (`... in function call`,
  `... in method call`, `... in constructor call`, `... in staticmethod call`).
  Like Pylint it counts the `self` of a method and of a constructor as an
  argument, lets a parameter before `/` be named by keyword, and leaves a
  keyword to `**kwargs` when the function has one (`W1117` reports the
  positional-only ones).

Not ported: a call whose callee needs types or a decorator model.

Known divergences from Pylint, where Pylint infers the callee:

* The callee is the one `W1114` resolves (see Batch B): a name bound once to
  a function of the file, `self.method` of the enclosing class, `cls(...)` in
  a `classmethod`, a class of the file with one `__init__`, such a class of
  another module of the project (arity rules only; its own `__init__`, no
  `__new__` along its bases) or an undecorated function that another module
  of the project defines once at its top level, reached by a name or a module
  alias bound once by an import (`from m import f`, `import m; m.f()`; not a
  module with a star import, nor one that only re-exports the function). A
  decorator changes nothing but `E1123` of a function or constructor, left
  out when it has one (Pylint looks for the keyword among what the decorator
  returns). A decorated or conditionally defined imported function, a class
  method, an alias of a method (`g = f`), a method that `self.name = ...`
  replaces, a function or an `__init__` defined twice and `self.__init__`
  (which Pylint does not judge) are not followed.
* A class is followed only when it has no `__new__` or metaclass (a class
  decorator changes nothing, as in Pylint) and every base is a builtin class,
  such a class of the file or such a class of another module of the project.
  A base from an installed package is not read.
* A call with a `**` or a `*` of anything but a name bound once to a list or
  tuple literal (`args = []` then `f(*args)` counts as no argument, whatever is
  appended later, as Pylint reads it) is skipped, where Pylint also unpacks a
  literal written in the call.
* A call to a `ctypes` type that astroid's brain gives a `__init__(self, value)`
  (`c_int()`, `c_ulong()`, also `ctypes.wintypes.DWORD()` and the other
  `wintypes` names of those types) reports the missing `value`.
  `self.__new__(Cls)` in a class reports the missing `cls` whatever its bases
  (astroid reads the `__new__` of an instance as `(self, cls)`); with other
  arguments it is not judged, nor is a method of an instance that a call
  returns. `f.__get__(a, b)` of a function of the file takes the parameters of
  `f`, defaulted or not, and `type` (astroid builds that signature); positional
  arguments only. `cls.method(...)` in a method of a class deriving from `type`
  or `abc.ABCMeta` is an unbound call, `cls` counted as a parameter. Left to
  type inference: a callee from another module (`ws.send(...)` of a socket built
  elsewhere), `self.get(...)` on a receiver not named `self`,
  `functools.partial` objects (`E1124`), `__get__` and `cls.method(x)` on an
  unbound method, a class returned by a call, and the stdlib signatures
  (`base64.urlsafe_b64decode(data, padded=True)`).
* `E1121` of `array.__new__(cls, 'i', args)` (a C type whose signature
  Pylint reads as `__new__(cls, typecode)`) and of `attrs.define` classes
  built from a base with its own `__init__` (Pylint reads the base's `__init__`
  and ignores the one `attrs` writes, so it reports a call that is valid) are
  not reported: the first needs the interpreter's signature, the second is a
  Pylint false positive.

## Batch S: private members, public methods and what an expression stands for

* `W0238` `unused-private-member`: a private function, variable or attribute
  of a class (a name with two leading underscores and not ending in two) that
  the class never uses. It follows Pylint's own reading, quirks included: a
  name used anywhere under the class counts, an attribute counts when read off
  `self`, `cls` or the class, a variable counts when any attribute is read off
  something that is not a name, and the class is judged again as part of each
  class that contains it. A function shows with its parameters as astroid
  prints them (`Cls.__f(self, a, b: int = 1, *rest, **kw)`).
* `R0903` `too-few-public-methods`: fewer public methods, counted over the class
  and its bases, than `min-public-methods` (default 2), plus the special methods
  the class itself defines other than `__init__`. Exceptions, enums,
  metaclasses, named tuples, typed dicts, dataclasses and `attrs` classes are
  exempt, and so is a class with an ancestor that
  `exclude-too-few-public-methods` matches.
* `E0239` `inherit-non-class`, `E0243` `invalid-class-object` and `W0143`
  `comparison-with-callable`: a base that is a function, a lambda, a literal
  or an instance of a class of the file; `X.__class__ = value` where the value
  is not a class; and `==`, `!=`, `is` and the like against a function, method
  or lambda.

Known divergences from Pylint, where Pylint infers a value or a base:

* `R0903` judges a class only when every base is a class of the project, a
  builtin exception, `object` or `type`. A class with another builtin base
  (`dict`, `list`), a base that is not found (an import from outside the
  project, `Generic[...]`, a starred base) is never reported, where Pylint
  counts the methods of the base. A base from a module the environment does
  not hold (a missing optional dependency) is left out of the count, as
  astroid cannot import it, unless the name is bound more than once. So is a
  name nothing binds when every `from m import *` reads such a module, and a
  name a star module binds to an empty class, and a name bound once to one
  (`Structure = ctypes.Structure`). So is a base that is, or is an attribute
  of, a parameter with no default (not the first of a method, not `*args`):
  astroid cannot infer it. A C class without public methods
  (`types.ModuleType`, `ast.AST`, `typing.ContextManager[...]`) counts none.
  Left to inference: a base that is a parameter with a default or a call, a
  base from a module that only the file's own directory or a package root of
  Pylint's `sys.path` could import, and C
  classes outside the method table.
* `E0239`, `E0243` and `W0143` answer for a literal, a function, a lambda, a
  class, a module, a call of a class of the file and a name bound once to one of
  those, also as an element of a tuple unpacking. A call of a function, a name
  bound twice, an imported name, a parameter and an attribute of an object have
  no answer and stay silent. `W0143` also takes the `__new__` of a builtin class
  for a function and a `getattr` of a parameter for no function. It stays silent
  where Pylint infers across modules: a function of an imported project module
  (`setters.validate`, `genericTypeValidator`), a function a call returns
  (`setters.pipe(...)`), a standard library function
  (`signal.default_int_handler`) and a value unpacked from a tuple of
  conditional expressions. Pylint also counts an uninferable operand as no
  function, which needs inference to tell from an operand it cannot see:
  `validator == genericTypeValidator` where `validator` is a `dict.get(...)`
  result.
* `E0239` prints a base written as an f-string as written; Pylint rewrites its
  quotes.
* `W0238` takes `type(self).__x` for the class itself, where Pylint infers the
  callee.

## Batch T: overrides and ancestors

The ancestors are those of Batch G (see its divergences for what is read).

* `W0222` `signature-differs`: a method that takes fewer default values than
  the method it overrides, with the same parameters otherwise; the filters of
  `W0221` apply (private names, setters, `@overload` stubs, constructors).
* `W0233` `non-parent-init-called`: in `__init__`, `Name.__init__(self)` where
  `Name` is a class that is not a direct base (builtins such as `object` and
  `Exception` included), and `self.__init__()`; a `Name.__init__` that is a
  direct base is counted once, as `W0231` counts it. `super().__init__` ends
  the check.
* `W0236` `invalid-overridden-method`: a method overridden as a property, or a
  property as a method, `classmethod` or `staticmethod`, and a method
  overridden as `async` or the reverse. `property`, `functools.cached_property`,
  `abc.abstractproperty`, a subclass of `property` and `@x.setter` count as
  properties.
* `W0246` `useless-parent-delegation`: a method whose only statement (after
  the docstring) calls the same method of `super()` with its own parameters,
  and that adds no decorator, default, annotation or `*args` the parent lacks.
  `__new__`, `__init_subclass__`, `__class_getitem__` and an `__hash__` next to
  an `__eq__` are left out. `super(Class, self)` and `super(self.__class__,
  self)` count.
* `R0901` `too-many-ancestors`: more ancestors than `max-parents` (default 7),
  the classes in Pylint's list of standard classes and in `ignored-parents`
  (and what they inherit) not counted. The builtin exceptions, `ABC`,
  `Generic`, `Protocol` and the builtin types count as Pylint counts them, and
  so does the `NoneType` astroid adds to a class that derives from `Enum` or
  `Flag`. `IOError`, `EnvironmentError`, `os.error`, `select.error` and
  `socket.error` are `OSError`.

Known divergences:

* A method of a builtin parent is not judged by `W0222`, `W0236` and `W0246`
  (Pylint reads its signature from the interpreter, `dict.__iter__` for
  instance); only a class with no base but the project's own and `object`,
  `ABC`, `Generic` or `Protocol` is compared with `object`, for `__init__`,
  `__repr__`, `__str__`, `__dir__`, `__sizeof__`, `__reduce__` and
  `__getstate__` written with `self` alone.
* `W0236` takes a decorator that is a function returning a property for an
  ordinary method.
  A parent written `Outer.Inner` is followed when `Outer` is a class of the
  file whose body binds `Inner` once as a class.
* `W0246` compares default values as written: `1`, `1.0` and `True` differ here
  and are equal in Python.
* `W0233` needs every base to be known, and skips a call whose target it
  cannot resolve.
* `R0901` leaves out a base it cannot read, as Pylint does for one it cannot
  infer, so a count can fall short; a module name bound by an import and again
  by an assignment (`try: import x` / `except ImportError: x = None`) is such a
  base.

## Batch U: final classes and methods, inheritance and instantiation

The bases are those of Batch G (see its divergences for what is read).

* `E0240` `inconsistent-mro`: bases that cannot be merged into a method
  resolution order, as `class C(A, B)` with `B` derived from `A`, and a class
  that derives from one that is inconsistent. `object` before another base
  counts. A base that cannot be read is left out, as Pylint leaves out one it
  cannot infer, so the rule still reports the bases it can read. Duplicate
  bases are `E0241`, which is not ported.
* `E0244` `invalid-enum-extension`: a direct base that derives from `Enum`
  (`IntEnum`, `Flag`, `StrEnum` included) and holds a member. A member is any
  name of its body bound only by assignments, the first with a value, other
  than `_ignore_`; so `_order_ = ...` counts, and `kind: int` does not.
* `W0240` `subclassed-final-class`: one message per direct base decorated
  with `typing.final` (or `typing_extensions.final`, called or not), also for
  `Base[int]`.
* `W0239` `overridden-final-method`: a method that overrides the one that its
  closest ancestor binds, when that one is decorated with `typing.final`.
  `W0239` and `W0240` stay silent for `py-version` below 3.8.
* `E0110` `abstract-class-instantiated`: a call of a class that still has a
  method decorated with `abc.abstractmethod`, `abstractproperty`,
  `abstractclassmethod` or `abstractstaticmethod` after the overrides of its
  MRO, when its metaclass is `ABCMeta` itself, declared or by deriving from
  `ABC`. A call inside the body of that class, or of a function in it, is
  left alone, and so is a `Protocol`.

Known divergences:

* `E0240` does not know the builtin classes other than `object`, so
  `class A(int, bool)` and `class A(dict, OrderedDict)` pass.
* `E0110` and `W0239` name a class only as an import, a class of the file or an
  attribute chain resolves it; a class held by a variable (`Alias = Base`), a
  parameter or a re-export (`from .compat import final`) is not seen. A class
  with a base it cannot read has no MRO, so `E0110` skips it.
* `E0244` finds a member in plain assignments, the way astroid's `Enum`
  support does, and does not follow a `for`, `with` or `except` that binds one.
  An enum the standard library builds without an `Enum` base in its source
  (`http.HTTPStatus`, made by `@_simple_enum`, and `signal.Signals`, made by
  `_convert_`) is not seen.

## Batch V: overlapping exceptions, unpacking and the result of a call

* `W0714` `overlapping-except`: two classes of one `except` tuple (or list)
  that are the same class (`ValueError and ValueError are the same`,
  `IOError and OSError`) or where one is an ancestor of the other, one message
  per pair. It belongs to `pylint.extensions.overlapping_exceptions`, so it
  reports only when `load-plugins` names that module. The hierarchy is the
  builtin exceptions and the classes of the file; `builtins.X`, `os.error`,
  `select.error` and `socket.error` are `OSError`, and `socket.timeout` is
  `TimeoutError`. `bad-except-order` (`E0701`) reads the same names now.
* `E1111` `assignment-from-no-return` and `E1128` `assignment-from-none`:
  `x = f()` where `f` is a function of the file, or of another module of the
  project that the file imports by name, with no `return`, or with
  `return` statements that all return nothing or `None`. Like Pylint it
  leaves out a decorated function, an `async` one, a generator, one whose body
  is a lone `raise` or ends in a `raise` with no `return`, and one whose first
  statement raises `NotImplementedError`. `E1111` names the function and
  suggests `sorted(...)` or `reversed(...)` after `sort` and `reverse`, or what
  `known-side-effects-only-functions` pairs. Only an `Assign` counts: an
  annotated assignment and `x += f()` do not.
* `W0632` `unbalanced-tuple-unpacking` and `W0644` `unbalanced-dict-unpacking`:
  `a, b = value` whose number of names differs from the items of `value`, and
  `for a, b in value.items()` (or `.keys()`, `.values()`) whose names differ
  from the items of every key or value. `value` is a tuple, list, set or dict
  display, a name bound once to one, a call of a function of the file whose
  only `return` returns one, or the view of such a dict. `self.x` and `cls.x`
  in a method follow to the class attribute `x = (1, 2)`, and `self.m()` to
  the undecorated method `m` of an ancestor, when every class of the MRO is
  read in the file, none has a builtin base and none stores `.x`. A slice of the
  `*args` of the enclosing function holds no value (`a, b = args[:2]` says
  `right side has 0 values`). The tuple rule words
  the display as `'(1, 2, 3)'` when it sits on the line of the assignment and
  as `defined at line N` otherwise. A starred name on the left, an abstract
  enclosing class, a starred element in the display and a dict with `**other`
  are left out, as in Pylint. A loop over the dict itself is not judged:
  Pylint leaves it to `dict-iter-missing-items` (`E1141`).

Not ported: `E0633` `unpacking-non-sequence` needs the type of a value.

Known divergences from Pylint, where Pylint infers a class or a callee:

* `W0714` leaves out a class held by a variable (`Alias = ValueError`), a
  class the file defines twice and one imported from another module; a local
  class counts the builtin and local bases it can read as its ancestors.
* `E1111` and `E1128` take the callee that `W1114` resolves (see Batch B and
  Batch R), a constructor aside: a name bound once to a function of the file
  or imported from another module of the project, or `self.method` of the
  enclosing class, also one that a base class of the
  project, of the standard library or of an installed package defines (the
  bases must all be known, and a `.pyi` base is left out). A method called as
  `Class.method(x)`, a name that the function binds again further down, a
  builtin method such as `lst.sort()` and `functools.partial(...)` are not
  followed, where Pylint infers the callee.
* `W0718` follows no alias that is set in a function, a class or a loop, and
  `E0701` knows no exception of an installed package: both need the
  inferred class, as do `W0125`, `W0143`, `E0712`, `E1102` and `E1124` for the
  value they judge. `R0902` counts the attributes of `self` only.
* `W0223` can differ on a whole run: Pylint reports a class there that it
  does not report on the file alone, from the state of astroid's import cache.
* `W0632` and `W0644` follow no more than the displays above. A name bound twice
  (Pylint takes the nearest assignment), `dict(...)`, `list(...)`, `tuple(...)`,
  a conditional expression, a slice or a sum, an attribute set in a method
  (`self.v = ...`) or of a class with a base from another module or a builtin,
  a property, a classmethod, a lambda, `self` read in a nested function and a
  method called as `Class.method()` are not judged, nor is a slice of an
  attribute that `__init__` sets to `[]` (`s = self.stack[:]; return s`) or a
  callee that another module defines. A
  display with a starred element, such as `(*t, 1)`, and a dict with
  `**other` are left out, where Pylint expands them, and an
  f-string inside the display is written with double quotes in the message.
  A list that a method returns after other statements emptied or filled it
  (`args = self.popall()`), a slice of `*args` (`F, A = B[:2]`) and a value
  that another module returns are not judged either: Pylint counts the
  elements it infers there.

## Batch W: docparams

The messages of `pylint.extensions.docparams`. They report only when
`load-plugins` names that module, and only then do their msg-ids (`W9005`,
`W9006`, `W9008`, `W9010` to `W9021`) answer to them: otherwise a plugin rule
declaring one of them as an alias owns it (see Batch K). A docstring is read in
the Sphinx, Epytext, Google or NumPy style, whichever has the most matching
sections (ties go to the first of that list, and no match to
`default-docstring-type`), with Pylint's own patterns.

* `W9015` `missing-param-doc` and `W9016` `missing-type-doc`: a parameter that
  the parameter section has no description (or no type, and no annotation) for.
  `self`, `cls`, the names that match `ignored-argument-names` and, for the
  type, `*args` and `**kwargs` are not asked for; `*args` may be documented
  without its asterisks.
* `W9017` `differing-param-doc` and `W9018` `differing-type-doc`: a documented
  name that the signature does not have.
* `W9005` `multiple-constructor-doc`: parameters documented in the class
  docstring and in the one of `__init__` (or `__new__`). The constructor's
  parameters are also judged against the class docstring, and the class
  docstring may stand for the constructor's.

* `W9006` `missing-raises-doc`: an exception that a `raise` of the function
  names and the docstring does not document, judged one `raise` at a time
  (so two `raise`s of one class report twice). The documented name may be an
  ancestor of the raised class (`Exception` for `ValueError`), and a dotted
  name counts by its last part. An exception that the nearest `try` around the
  `raise` handles by name (`except ValueError`, not `except Exception`) or a
  `contextlib.suppress(...)` around it names is left out, and so is
  `NotImplementedError` in an abstract method. A bare `raise` in an `except`
  re-raises the handler's classes. A setter without a docstring is judged
  against its property's.
* `W9011` `missing-return-doc`: a `return` with a value (not `None`) in a
  function whose docstring has no return section; the type line of a property's
  docstring stands for it. `W9013` `missing-yield-doc`: a `yield` in a
  function whose docstring has no yields section (a Sphinx docstring takes
  `:returns:`). Each `return` or `yield` reports on its own.
* `W9008` `redundant-returns-doc`: a return section in a function with no
  `return` that gives a value (counting the ones in nested functions), unless
  it is a generator in a style without a yields section, or abstract.
  `W9010` `redundant-yields-doc`: a yields section in a function that is not a
  generator.

They read `accept-no-param-doc` (a docstring with no parameter section at all
is accepted when it is `yes`, the default), `default-docstring-type`,
`no-docstring-rgx` (default `^_`, so `__init__` is skipped unless the option
changes), `docstring-min-length` and `ignored-argument-names`; the new
messages read `accept-no-raise-doc`, `accept-no-return-doc` and
`accept-no-yields-doc` (all `yes` by default, so a docstring with no such
section is accepted). Pylint measures the length of a `raise` or a `yield` as
one line, so a positive `docstring-min-length` skips `W9006` and `W9013`
altogether. `For the parameters, see` in a docstring tolerates what is
missing. A function decorated `overload` is skipped.

* `W9012` `missing-return-type-doc` and `W9014` `missing-yield-type-doc`: the
  type line of a return or yields section is missing, which a return
  annotation or a function type comment stands for. They report with
  `W9011` and `W9013`, and under the same options.
* `W9019` `useless-param-doc` and `W9020` `useless-type-doc`: a parameter that
  `ignored-argument-names` ignores (default `_.*|^ignored_|^unused_`) has a
  description or a documented type. An annotation does not count for
  `W9020`.
* `W9021` `missing-any-param-doc`: every parameter of the function lacks both
  a description and a type; it replaces `W9015` and `W9016`. A docstring that
  documents no parameter is tolerated under the default `accept-no-param-doc`,
  so it shows with `no`. A method never gets it, since `self` is not asked for
  and so is never missing.

Known divergences from Pylint:

* A function's type comment (`# type: (int) -> str`) and one on each parameter
  are read and matched to the parameters by position, as Pylint does. Pylint
  drops every type comment of a file that holds a misplaced one (the file
  then parses without them); that is not reproduced.
* A method is a constructor's owner when the nearest class around it is the
  class; Pylint resolves the same frame, with astroid's inference of
  decorators left out here (`overload` is taken by its last name).
* A raised exception is the class that a name, an attribute (`mod.Error`,
  `self.Error`, `Class.Error`) or a call names directly: a builtin (`IOError`
  and `socket.error` are `OSError`), a class of the file or one the project
  holds. Pylint also infers `raise error` for a variable that holds an
  instance, a name assigned a class, a function that returns an exception and
  an instance attribute (`raise self.predicate()`); those are not reported
  here. Pylint cannot see a name imported in a branch it takes as
  unreachable (an `else` of a `sys.version_info` test), so it does not judge
  a bare `raise` under `except mod.Error` there, where this reports it. A
  class that only a C module defines (`queue.Empty`) is not followed.
* A property is a decorator named `property`, `functools.cached_property`,
  `abc.abstractproperty` or a class that derives from them; a function that
  returns a property is not followed. A setter finds its property among the
  methods of its own class, not those of its ancestors.
* A `yield` in a `lambda` is not checked: Pylint crashes on it.

## Batch X: project-wide rules and isinstance

* `W1116` `isinstance-second-argument-not-valid-type`: `isinstance(x, y)` where
  `y` is certainly no class: a literal (number, string, `None`, list, dict,
  set, lambda), a function, a module the project holds, an instance of a class
  of the file, a tuple with such an element, or `a | b` with such a side other
  than `None`. A name bound once to one of those counts too, also inside a
  tuple.
* `E0401` `import-error`, `E0402` `relative-beyond-top-level` and `E0611`
  `no-name-in-module`: an import that no module answers, a relative import
  that climbs above the top package, and a name that the module imported does
  not bind. The modules are found the way astroid finds them: the
  directory above each linted package, `PYTHONPATH`, the standard library and
  the `site-packages` of `VIRTUAL_ENV`, with packages ahead of compiled modules
  ahead of `.py` files ahead of namespace directories. `ignored-modules`,
  `analyse-fallback-blocks`, `source-roots`, `extension-pkg-allow-list` and
  `unsafe-load-any-extension` apply. An import under `if TYPE_CHECKING:`, under
  a `sys.version_info` or `six.PY2/PY3` test, in a `try` that catches
  `ImportError`, in `with suppress(ImportError)` or beside a fallback import is
  not reported, as in Pylint. `__main__` is always found, as it is in
  `sys.modules` of the interpreter that runs Pylint.
* `R0401` `cyclic-import`: the cycles of the imports among the linted modules
  (and the packages they import from), each reported once as
  `Cyclic import (a -> b)`. An import under `if TYPE_CHECKING:` or one with
  `# pylint: disable=cyclic-import` leaves the graph. The graph is Pylint's:
  an import in a function or a `try` counts, `from p import name` stands for
  `p.name` when that is a module and for `p` when it is not, and an edge that
  is also imported under `TYPE_CHECKING` or with the disable is dropped.

* `R0801` `duplicate-code`: lines that two or more linted files share, found
  as `symilar` finds them: the stripped lines of every file, a window of
  `min-similarity-lines` of them (default 4) hashed, and the windows that
  match merged into runs. Reported once, at line 1 of the last file, as
  `Similar lines in N files` followed by `==module:[start:end]` for each file
  and the shared lines. `min-similarity-lines`, `ignore-comments`,
  `ignore-docstrings`, `ignore-imports` and `ignore-signatures` apply.
  A line under `# pylint: disable=duplicate-code` leaves the comparison, and
  so does a file whose configuration (`per-file-ignores`) turns the rule off.

These five rules need the whole run, not one file. They run after the
per-file pass and keep their result in one cache entry: see
[Result cache](result-cache.md).

Known divergences from Pylint, where Pylint infers the second argument:

* `W1116` leaves out every expression that `E0239` cannot classify without
  types: a call of a function, an attribute of an object, a name bound twice,
  a parameter, a subscript such as `list[int]`. A union held by a name is
  never judged (Pylint infers it only when both sides are classes).

Known divergences of the project rules (`E0401`, `E0402`, `E0611`, `R0401`,
`R0801`):

* Without `VIRTUAL_ENV` the standard library and the installed packages are
  unknown, so an absolute import is never reported as missing and a name is
  never judged missing in them; relative imports and the project's own modules
  still are. Set `VIRTUAL_ENV` to the environment Pylint runs in to match it.
* `R0401` is reported once per cycle at line 1 of the file `os.walk` reaches
  last, as `R0801` (below); Pylint attaches it to the file it checked last,
  in filesystem order. The cycles and their text agree.
* `R0801` takes the files in name order and attaches the messages to the one
  `os.walk` reaches last (a directory's files, then its subdirectories, in the
  order the file system lists them), as Pylint does; that order is the file
  system's, so it differs between machines. Pylint prints the
  lines of any one of the files (a set's iteration order); this prints those
  of the first. Several `R0401` or `R0801` findings of one run come out
  ordered by message, since both sit at the same position. The hash of a
  window is the sum of its lines' hashes, as in Pylint 4.1.3, so a window of
  the same lines in another order matches too. The `R0401` cycles are found
  by a search whose order depends on the order of the files, so a project
  with many overlapping cycles may list other cycles (over the same modules)
  than Pylint. On the same graph (equal edge for edge to Pylint's on the
  corpora measured, but for the points below) the search with the neighbors
  of a module in random order finds
  from 123 to 136 cycles where this finds 129, so a count that differs by a
  few percent is Pylint's own run-to-run spread (its sets iterate in the
  order of a hash seeded for each run), not a missing edge.
* The rules that read another module of the project (`W0614`, `W0237`,
  `W0621` and `E0602` through a star import, `E0202`) look for it as astroid
  does: in the directory above the top package of each argument of the run
  (a source root when one holds it) and in the directory of the file. They do
  not look in the other directories above the file, so a module of a subtree
  that is no package is not found from outside it (`E0401`), and none of
  these is reported. A relative import is read under the module name Pylint
  gives the file from the root of its argument. A run on one file has the
  directory above the top package of that file as its root.
* `W0631` is not reported when Pylint infers the iterable as a non-empty
  sequence. That holds for `for x in f()` where `f` returns a tuple or list
  literal. ferrolint follows literals, names bound to them, `self`
  attributes, `sys.path`, `sys.meta_path`, `sys.modules` and `os.environ`,
  and a display a class of the project holds (`pkg.Cls.TABLE`).
* `R0902` counts an attribute that another class assigns through an inferred
  instance (`self.worker_fixup.worker = w`, a property returning the class);
  ferrolint counts the `self.x` of the class itself.
* A file system that ignores case (macOS, Windows) opens `TTFont.py` as
  `ttFont.py`, and astroid tests `isfile`: a submodule or compiled module is
  found by any spelling there, as in Pylint, so `from pkg import TTFont` is an
  edge to `pkg.TTFont`, which is no module of the graph. On a file system that
  tells the cases apart it is an edge to `pkg`, as Pylint finds it there.
* Pylint lints `.pyi` files when it is given a directory, and knows a linted
  module by its name. So does ferrolint: a stub found under a directory is a
  module (`E0401` and `E0611` answer from its names, and its own imports are
  checked), and the per-file rules run on it as on a `.py`, except where
  Pylint skips a stub: `W0231` and `W0233` (its `__init__` body is `...`), and
  `E1111` and `E1128` (astroid's module is not "fully defined" unless it
  comes from a `.py`). `W0613` is Ruff's. Pylint's `astroid` does not look
  for stubs, so `import pkg.stub` and `from pkg.stub import x` are an edge to
  `pkg`, not to `pkg.stub`. Given only `.py` files, Pylint finds no stub, and
  neither does ferrolint. A `.py` shadows a `.pyi` of the same name. An
  `import __main__` is not an edge (Pylint has the module of the run).
  A directory with only `__init__.pyi` is a namespace package, as in Pylint
  (a `.pyi` there is a module by its own name), checked on a directory run;
  with the directory itself as an argument, Pylint answers `E0401` for
  `import d.sub` and ferrolint differs (not reproduced).
  A stub's `from m import *` of a module that only a stub holds gets no
  `W0614` here; Pylint reports it or not by the order of its run.
* A package found on the search path with only an `__init__.pyi` (a compiled
  module's stub, `pydantic_core._pydantic_core`) is read from the stub, as
  astroid's finder does, also when a compiled module of that name sits beside it.
* A file with `raise;` or `except E:raise;stmt` (a `raise` ended by a `;`) is a
  syntax error to the parser of ruff 0.17.1, so it has no imports in the
  graph; Python and Pylint accept it.
* Pylint reads astroid's brain plugins, which add names to some modules
  (`hashlib`, `six`, `numpy` and others); those modules accept any name here.
  `collections` is not one: its brain adds names the source holds, so
  `from collections import Mapping` is reported.
* A `from m import *` of a source that does not parse (such as `raise;` above)
  binds names that cannot be told, so the module that star-imports it is not
  judged for `E0611`.
  A name that a module imports from another is not followed to what it infers
  to.
* `TYPE_CHECKING`, `ImportError`, `typing` and `suppress` are recognized by
  their spelling and the file's own imports, where Pylint infers them.
* The cache entry stamps the directories and files the resolution read; a
  `.pth` file edited in place without a size or time change is not noticed.
