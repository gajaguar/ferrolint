---
type: reference
title: Rules answered by ty
description: The nine Pylint operation rules that exist only in a build with the ty feature, the single-type criterion that keeps them close to Pylint, and the differences that remain.
tags: [rules, inference, ty]
status: draft
---

# Rules answered by ty

A build with the `ty` cargo feature (`cargo build --features ty`, or
`make release-build FEATURES=ty`) adds nine Pylint rules that need the type of
a value. Without the feature they do not exist: `select` and `ignore` do not
know them. ty starts only when one of them is enabled, and it reads
`VIRTUAL_ENV` to find `site-packages`. If it cannot start, `ferrolint` prints a
warning and the nine rules report nothing.

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

## Where it differs from Pylint

* ty trusts declarations and Pylint infers values. A function annotated
  `-> bool` that returns a tuple is `E0633` for ty at `a, b = f()`; Pylint
  infers the tuple and is silent. The code is wrong, but it is not what
  Pylint reports.
* Pylint ignores the members it cannot infer, ty does not: `x: dict | None`
  from `deepcopy(...)` is reported by Pylint and not here.
* Pylint infers a default (`x = None`) that ty sees as `Unknown` or a union,
  and a `# pylint: disable` hides most of the rest.
* `E1131` is reported where Pylint does not infer (`1 + None`). The `E1130`
  message names ty's type (`Literal["a"]`), where Pylint names `str`.
* Extra findings that wait for the Django plugin and are not worked around: a
  `JSONField` or `ArrayField` read as the field and not as its value,
  `factory_boy` declarations (`Faker`, `LazyFunction`, `LazyAttribute`) read as
  the declaration and not as the value of the model, and an attribute annotated
  as an instance to which the class is assigned.
* The bundled `not-callable` plugin (`CAL001`, alias `E1102`) is not bundled
  in a build with `ty`, which answers `E1102` itself. A `[[plugins]]` entry
  that loads it fails with the alias error.
* The cache does not notice a change in another module that alters an
  inferred type; use `--no-cache` after one.
* `*x` in a call and `yield from x` are not checked for `E1133`.

## Why ty does not share the host's tree

ty infers over the tree its own database parses, and the host numbers the
nodes of the tree it works on, so the file is parsed twice when ty is on.
