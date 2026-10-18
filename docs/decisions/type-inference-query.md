---
type: decision
title: Type inference query
description: How contract 0.2.1 adds the type inference query as a new interface, what the host infers, and why an unresolved answer is the default.
tags: [plugins, inference]
status: stable
---

# Type inference query

[FR-11](../requirements/type-inference-query.md) asks for a type query
next to the phase 1 scope queries, without breaking the plugins written
against them.

## Shape

Contract 0.2.1 adds an `inference` interface, marked
`@since(version = 0.2.1)`, with one function:
`infer(node) -> inferred`. An `inferred` holds a type `name`, an
`origin` and a `resolved` flag.

* A new interface, not a new function on `semantic`: an interface is the
  unit a component imports. A plugin built against 0.2.0 does not name
  it, and wasmtime matches its `semantic@0.2.0` import to the host's
  `0.2.1` by semver, so it loads unchanged. A host of 0.2.0 refuses a
  plugin built with SDK 0.2.1, since the SDK's `Context::infer` is
  linked into every plugin. A test loads a component whose interface
  names are rewritten to `@0.2.0`.
* `origin` says where the type came from (a literal, a builtin, an
  import, a class or function definition by node, a call by node, an
  annotation by node), so a rule can tell `x = 1` from `x: int`.
* `resolved` is false whenever the engine gave up. The name is then
  `unknown`, except that a `from` import keeps its module in the origin.

## What the host infers

`crates/ferrolint/src/infer.rs` follows, at most 16 steps deep, and
answers resolved only when the type holds on every path:

* Literals and displays, `not` (`bool`), `+`, `-` and `*` on `int` and
  `float`, `/` on them (`float`), and `+` on two `str`, `bytes`, `list`
  or `tuple` values.
* Names bound once in their scope, through the scope model's `resolve`:
  a builtin class is `type[int]`, another builtin a
  `builtin_function_or_method`, an `import` a `module`, a class
  `type[C]`, a function `function`, an annotated assignment its annotation,
  and a single-target assignment its value.
* Calls: calling `type[C]` gives `C`, and calling an undecorated,
  synchronous function with a plain return annotation gives that
  annotation.

Anything else is unresolved:

* a name bound twice in its scope (`try`/`except`, `if`/`else`), since
  either value may reach the use;
* a comparison, since `__eq__` may return anything;
* an annotation more elaborate than a dotted name, even when the value
  is a literal (`x: Callable | None = None` is not `NoneType`);
* a call of an `async` or decorated function;
* attributes, subscripts, tuple unpacking, `from` imports and
  parameters.

One query infers each expression once, so a cycle (`x = x + x`) ends
unresolved and shared operands cost no extra work. The engine works on
the flattened AST and the scope model the host already builds for
`semantic`, so a file costs no extra parse.

## Consequences

* A plugin reports on a type only when `resolved` is true; the SDK's
  `Semantic::infer` defaults to unresolved, so a native test that does
  not stub it gets silence, not a guess.
* `plugins/not-callable` (`CAL001`, Pylint `E1102`) is the sample. Its
  tests in `crates/ferrolint/tests/plugins.rs` cover reports on resolved
  builtin instances and silence on every unresolved form above.
* Widening the engine (attributes, `from` imports across files, unions)
  turns unresolved answers into resolved ones. Plugins see more findings,
  but the contract does not change.
