---
type: reference
title: Rules answered by ty
description: The ten Pylint rules that exist only in a build with the ty feature, the single-type criterion that keeps them close to Pylint, and the differences that remain.
tags: [rules, inference, ty]
status: draft
---

# Rules answered by ty

A build with the `ty` cargo feature (`cargo build --features ty`, or
`make release-build FEATURES=ty`) adds ten Pylint rules that need the type of
a value. Without the feature they do not exist: `select` and `ignore` do not
know them. ty starts only when one of them, `C0103` or a plugin is on, and it
reads `VIRTUAL_ENV` to find `site-packages`. If it cannot start, `ferrolint`
prints a warning and the ten rules report nothing; `C0103` and the plugins
fall back to what the host infers by itself.

| Rule                                       | Reports                        | Node                                         |
| :----------------------------------------- | :----------------------------- | :------------------------------------------- |
| `E1136` `unsubscriptable-object`           | `x[0]`                         | a subscript that is read                     |
| `E1137` `unsupported-assignment-operation` | `x[0] = 1`                     | a subscript that is assigned                 |
| `E1138` `unsupported-delete-operation`     | `del x[0]`                     | a subscript that is deleted                  |
| `E1102` `not-callable`                     | `x()`                          | a call                                       |
| `E1133` `not-an-iterable`                  | `for _ in x`, `[_ for _ in x]` | the iterated value                           |
| `E0633` `unpacking-non-sequence`           | `a, b = x`                     | an assignment to a tuple or list             |
| `E1135` `unsupported-membership-test`      | `1 in x`, `1 not in x`         | a single `in` comparison                     |
| `E1130` `invalid-unary-operand-type`       | `-x`                           | a unary operator                             |
| `E1131` `unsupported-binary-operation`     | `x + y`, `x += y`              | a binary operator or an augmented assignment |
| `E1101` `no-member`                        | `self.x`, `cls.x`              | a read of a member `self` or `cls` lacks     |

`E1126` and `E1127` (a bad sequence index or slice) are not covered: ty has no
lint for them.

## The criterion

ty reports far more than Pylint (513 unsubscriptable values against 11 on one
codebase), mostly unions where one member fails. A rule starts from a ty
diagnostic, finds the node by its range and its shape (a `for` is not an
unpacking, `in` is not a unary operator, `del` is not a read), and keeps it
only when the operand has a **single concrete type**. A union, an
intersection, an unknown or unresolved type, a type variable, `object` and a
special form such as `Annotated[...]` are all silent, as Pylint is. The shape
of the node, not the text of the message, tells the rules apart.

### `E1101` reports only on `self` and `cls`

ty types an annotated parameter or attribute by its declaration, where Pylint
infers a value, so any other receiver disagrees with Pylint on most lines (on
one Django codebase 80 to 95% of them). `self` and `cls` are different: ty
types them as the enclosing class, and `E1101` keeps a read of a missing member
there when the class is closed. A class is closed when no ancestor defines
`__getattr__` or `__getattribute__`, sets a metaclass or has a base ty cannot
resolve. It is silent as Pylint is for a name that matches `mixin-class-rgx`
(`ignored-checks-for-mixins`), a name in `generated-members` or `ignored-classes`,
and a read inside a `try` whose handler catches `AttributeError`. The message
carries Pylint's hint (`; maybe 'x'?`), read from `missing-member-hint`,
`missing-member-hint-distance` and `missing-member-max-choices`.

A class that declares `__slots__` keeps the names it assigns as `self.x = ...`
as members: ty drops the ones the slots omit and Pylint does not.

Measured against Pylint run with `--enable=no-member` (a Django codebase whose
own `.pylintrc` disables it, and a set of installed packages): the codebase
727 lines agree, none only here and 690 only Pylint (51% of 1417); the packages
327 agree, 8 only here and 94 only Pylint (78% of 421). On the codebase it takes
3.3 s and 1.6 GB with five threads. Of the 690, none is a `self` or `cls`
receiver except 15 (13 in a class with a plain metaclass); the rest read a
member of another object or a value ty does not flag. What stays with Pylint:
members of another object (an instance of a class, a module), Django models and
fields (the Django plugin), and a class with a plain metaclass.

The last is left silent on purpose. Resolving the metaclass and checking that it
has no `__getattr__` agrees on 17 more lines, but reports 110 on Django models
(`ModelBase` is plain), where `pylint_django` makes Pylint silent. Telling
`ModelBase` from a serializer's metaclass would take the name of a package.

## Where it differs from Pylint

* ty trusts declarations and Pylint infers values. A function annotated
  `-> bool` that returns a tuple is `E0633` for ty at `a, b = f()`; Pylint
  infers the tuple and is silent. The code is wrong, but it is not what
  Pylint reports.
* Pylint ignores the members it cannot infer, ty does not: `x: dict | None`
  from `deepcopy(...)` is reported by Pylint and not here.
* Pylint infers a default (`x = None`) that ty sees as `Unknown` or a union,
  and a `# pylint: disable` hides most of the rest.
* `E1131` is not reported for `xs += [...]` or `xs |= {...}`: ty types
  `xs = [A]` as a list of `A` only and rejects the other element types, which
  Python accepts and Pylint does not report.
* `E1131` is reported where Pylint does not infer (`1 + None`). The `E1130`
  message names ty's type (`Literal["a"]`), where Pylint names `str`.
* Without `django-stubs` in the environment, ty reads a `JSONField` or
  `ArrayField` as the field and not as its value. Install `django-stubs` in
  the `VIRTUAL_ENV` ty reads: on one Django codebase it cut `E1137` from 156
  to 3 and `E1136` from 69 to 12. What stays is `factory_boy` declarations
  (`Faker`, `LazyFunction`, `LazyAttribute`) read as the declaration and not
  as the value of the model, a few fields ty still types as the field, and an
  attribute annotated as an instance to which the class is assigned. These
  wait for the Django plugin and are not worked around.
* The bundled `not-callable` plugin (`CAL001`, alias `E1102`) is not bundled
  in a build with `ty`, which answers `E1102` itself. A `[[plugins]]` entry
  that loads it fails with the alias error.
* The cache does not notice a change in another module that alters an
  inferred type; use `--no-cache` after one.
* `*x` in a call and `yield from x` are not checked for `E1133`.

## What ty completes in the other rules

Two places ask ty for the value of an expression that the host cannot follow,
and keep the answer only when it is one concrete type:

* `C0103` takes a name, attribute or call that is a class (`orig = queue.LifoQueue`)
  for a class and checks its name as PascalCase, also for a variable of a function,
  an element of a tuple (`tasks, Task = self.tasks, self.Task`) and an `if` of two
  classes (the last needs no ty), and for a lower-case import that is a class
  (`from itertools import cycle`). A function or a literal is not
  taken: ty's are wider than Pylint's `Const` (an enum member is a literal to ty
  and an instance to Pylint). A class that its own module binds again
  (`HTTPSConnection = DummyConnection` under `if not ssl`) is left alone: Pylint
  infers two values there. On a set of installed packages it closed 2 of 21
  lines only Pylint reports and added none; on a Django codebase it changed
  nothing. One difference remains: `threading.Lock` is a class to ty and a
  function to Pylint.
* `W0201` takes the receiver `self.Connection()` of an attribute store
  (`conn = self.Connection(); conn.client = self`) for an instance of a class
  of the same file when a class attribute rebinds it
  (`Connection = Connection`), which the host's own lookup does not follow.
  ty must name the class and say it is written in this file. On a set of
  installed packages it closed 3 of 9 lines only Pylint reports and added
  none; on a Django codebase it changed nothing.
* A plugin's `inference` query answers `type[Name]`, `function` or `module` when
  the host's own engine is unresolved. The contract does not change.

The class rules (`W0221`, `W0237`, `W0223`, `R0901`, `R0903`, `E0203`) are
not completed: the 7 lines only Pylint reports in them on a set of installed
packages (none on a Django codebase) sit in classes whose bases the host
reads, or whose base ty types as a union or `Unknown`, so naming the class
through ty closes none. `W0125`, `W0143`, `C1803` and `E0701` are not
completed either: on the same packages every line only Pylint reports was an
attribute that ty types as `Unknown` or a union, and outside its crate ty
exposes no class hierarchy for `E0701`.

## Why ty does not share the host's tree

ty infers over the tree its own database parses, and the host numbers the
nodes of the tree it works on, so the file is parsed twice when ty is on.
