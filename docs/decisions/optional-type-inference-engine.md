---
type: decision
title: Optional type inference engine
description: Why ty is not embedded yet, what a trial showed, and the shape it will take later - a cargo feature that users can turn on and that is off by default.
tags: [inference, rules]
status: draft
---

# Optional type inference engine

The native Pylint rules cover everything that is decidable from syntax and
scopes. What is left needs types: argument counts of a call to a function
defined elsewhere (`E1120`), subscripting, iterating and calling a value
(`E1136`, `E1135`, `E1102`), and a few more. This note records why the
engine for them, [ty](https://github.com/astral-sh/ty), is not part of the
default build, and what is decided for later.

## What a trial showed

A throwaway crate embedded ty's crates, at the same tag as the Ruff crates
already linked, and ran them over a codebase of 3,445 Python files.

* **It builds and it is fast.** It compiled next to the WASM runtime with
  no version conflict. Inferring 1.5 million expressions took 2.2 s, a
  full `check` 2.1 s, with a peak of 1.56 GB, under the 2 GB budget with
  little margin.
* **Imports resolve.** Only 4 unresolved imports in the whole tree, which
  makes `import-error` and `no-name-in-module` possible.
* **Roughly a quarter of the expressions stay `Unknown`** (23%). A rule
  that stays silent on `Unknown` says nothing there.
* **It is far noisier than Pylint.** The equivalent ty rules reported
  513 non-subscriptable values where Pylint reported 11, 97 non-iterable
  against 1, and 52 non-callable against 3. The causes are values that can
  be `None`, unions with a member that is not iterable, descriptors on
  model fields, and factory libraries whose call returns the factory, not
  the object it builds. A rule built on ty has to follow Pylint's
  criterion, not ty's.
* **ty reads its own AST.** It infers only over the tree its salsa database
  parses, so the host would parse through ty, or twice.

## Why it is not in the default build

* **`cargo-deny` fails.** The dependency graph brings licenses the
  allow-list does not accept (MPL-2.0 and CC0-1.0) and three advisories on
  a `zip` release, one of them RUSTSEC-2026-0367, reached through
  Ruff's `ruff_db`. `make deny` runs in `make check` and in CI.
* **The gain is small.** On a large codebase with its Pylint configuration,
  82 of 1704 findings of a run with no suppression comments need
  inference, 5%, and nearly all of them are already silenced there by
  suppression comments. Every other finding is reported natively.
* **The cost is paid by everyone.** Memory, build time and binary size would
  grow for users who never need the rules.

## Decision

* Do not embed ty in the default build. The step is postponed to a later
  stage of development.
* When it is built, it is a cargo feature that users can turn on and that
  is off by default. A binary built without the feature has no ty code and
  keeps the current dependency graph, so the license and advisory checks
  stay as they are.
* The rules that need types report nothing, and are not listed, in a build
  without the feature. A configuration that names one of them is not an
  error, as for any rule the build does not know.
* The decision was weighed with a model-based evaluation of the options
  (embed now, a cheap syntactic subset, postpone, never): postponing won
  with a probability of 0.69. How to isolate the dependency (a cargo
  feature or a separate plugin) scored 0.40 and 0.36, too close to settle
  now.

## What the later stage has to settle

* The license and advisory policy for the feature's own graph: allow the
  two licenses and ignore the advisory with a reason, or keep the feature
  out of the checked graph.
* Whether the feature ships in the PyPI wheel or only in a separate build.
* Per-rule policies that follow Pylint: ignore types that may be `None`,
  stay silent on `Unknown`, and small host-side transformations for the
  dynamic patterns above.
* How to avoid parsing twice, given that ty infers only over its own tree.
* A budget: the trial's 2.8 s and 1.56 GB per run are the baseline.

## What the choice puts on future us

* The five percent of findings that need types are not reported until the
  feature exists; whoever depends on them keeps Pylint for those rules.
* `make pylint-compare` keeps listing them as Pylint-only differences, so
  the gap stays measured.
