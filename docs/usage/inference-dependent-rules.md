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
  `E1124` and `E0712`, and `R1710` or `W0101` (a method that never
  returns, called on a value that is not `self`: `ctx.exit()` where `ctx`
  comes from `with self.make_context(...) as ctx`).
* **The value a name stands for.** `C0103` (a constant that comes from a
  call or an attribute chain, such as `ctypes.c_size_t`), `W0632` (an
  iterable that is a function result or another module's dict), `W0143` and
  `W0125` (an operand of a comparison or a condition), `W0614` (an `__all__`
  that is `list(m.__all__)` or a name: ferrolint reports nothing for the
  file).
* **A class that is not a plain class statement.** `R0903` (a base that a
  call returns), `W0223` (methods of a C class such as `io.IOBase`), `R0901`
  (an ancestor list that doubles through a C class) and `W0237` or `W0221`
  on a base that comes in through a star import. Two more: `R0903`, when a
  base is the attribute of a project module bound in a `try`
  (`staticfiles.finders.Base`, where `finders` is imported in the `try` and
  set to `None` in the `except`), which Pylint infers and ferrolint takes
  for a module that is missing; and `W0221`, when a base name is bound again
  by a later `Name = Other` under an `if` (`HTTPSConnection = Dummy`), as
  astroid builds the MRO from the last value of the name.

## What is open, by group

Counts are the lines only Pylint reports on a corpus of installed packages
(the large set; the smaller set is in brackets when it has any). Each has one
example.

The type of a call result or attribute:

* `W0201` 32 [5]: `self._storage.cvar = _CVar()`, an instance reached
  through an attribute.
* `R0902` 6: `self.x = ...` through an instance a call returns.
* `E1120` 5 and `E1121` 4: `obj = cls()` where `cls` is a metaclass, and
  `type(self)(value)`.
* `E1102` 14 [3]: `self.callback()` where an attribute holds a value that is
  not callable, or `new_instance = cls()` in a class method.
* `E1124` 13: `f(worker, sig="X")` where `f` is bound to a
  `functools.partial` that already fills `sig`.
* `E0712` 6: `except autoretry_for:` where a name holds a tuple of classes
  a call returned.
* `E0203` 5: `self.__dict__.copy()` before the class sets `__dict__`, which
  needs the base class members.

The value a name stands for:

* `C0103` 68 [13]: `_CACERT_CTX = as_file(...)`, a constant that is the
  result of a call, or an alias such as `ctypes.c_size_t`.
* `W0632` 2: `a, b = unpack(x)`, a function of an extension module.
* `W0125` 11 [1] and `W0143` 11: `if self.auth:` and `url.port == 80`,
  where an attribute holds a value Pylint infers as a constant or a
  function.
* `C1803` 5: `result == {}`, which needs the type of `result`.
* `E0702` 2 [3]: `raise heartbeat_error[0]` where the list holds `None`.
* `W0631` 2: a loop variable used after a loop that may not run, in a file
  Pylint infers further than the flow here.

A class that is not a plain class statement:

* `W0223` 4 [11]: `class A(BaseHTTPResponse)`, where the base comes from an
  installed package whose abstract methods are inferred.
* `E0701` 3: `except StatusAuthnFailed:` after a class of an installed
  package that is its ancestor.

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
