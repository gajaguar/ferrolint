---
type: reference
title: Rules that wait for type inference
description: The native rules whose remaining differences from Pylint need real type inference, grouped by what the missing inference is, and the optional ty engine planned to supply it.
tags: [rules, inference]
status: draft
---

# Rules that wait for type inference

The native rules decide from syntax, scopes and the modules they can read.
Where Pylint reports because astroid infers a value, ferrolint stays silent
rather than guess: a missed finding costs less than a false one. The
differences that remain are of that kind. They cannot be closed rule by rule;
they close together once an inference engine answers the questions below.

The engine planned for this is [ty](https://github.com/astral-sh/ty), behind
an optional cargo feature that is off by default. It is not built yet. The
interface a rule would ask it through is in the
[type inference query](../decisions/type-inference-query.md) note.

## What the missing inference is

* **The type of a call result or attribute.** `W0201` (instances reached
  through an attribute, a list or a call), `R0902` (attributes assigned
  through an inferred instance), `E1120` and `E1121` (`type(self)(...)`, the
  `cls()` of a metaclass, signatures of installed packages), `E1102`,
  `E1124` and `E0712`.
* **The value a name stands for.** `C0103` (a constant that comes from the
  standard library, such as `os.sep`), `W0632` (an iterable that is a
  function result or another module's dict), `W0143` and `W0125` (an operand
  of a comparison or a condition), `W0614` (an `__all__` that is
  `list(m.__all__)` or a name: ferrolint reports nothing for the file).
* **A class that is not a plain class statement.** `R0903` (a base that a
  call returns), `W0223` (methods of a C class such as `io.IOBase`), `R0901`
  (an ancestor list that doubles through a C class) and `W0237` or `W0221`
  on a base that comes in through a star import.

## What does not wait for it

* `R0401` differs in the cycle it names, not in whether there is one. Pylint
  walks a set of module names, and the order of that set follows the hash
  seed of the run, so two Pylint runs disagree with each other.
* The rules that search imports from the arguments of the run need the
  run's roots, not inference; see the rule notes in
  [native rules](native-rules.md).

## How to use this note

A difference found against Pylint that falls under one of the groups above
is a known gap, not a bug to chase: record it under its rule in
[native rules](native-rules.md) and leave it. Anything that does not fall
under them is a bug.
