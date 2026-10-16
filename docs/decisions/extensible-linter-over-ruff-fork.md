---
type: decision
title: Extensible linter over a Ruff fork
description: ferrolint is a new Rust linter that runs only WASM-plugin rules next to Ruff, instead of forking Ruff or rewriting Pylint.
tags: [pylint, ruff, wasi]
status: stable
---

# Extensible linter over a Ruff fork

ferrolint exists because neither of the two off-the-shelf linters can do what
we need, so the decision was which new thing to build.

## Context

Pylint is the source of truth for many checks, such as the `PL*` rules Ruff
inherits, but it is too slow for agent loops and Bitbucket pipelines on large
codebases. Ruff is fast, but it has no plugin API —
[astral-sh/ruff#283](https://github.com/astral-sh/ruff/issues/283) has been
open for years and upstream refuses third-party rule plugins on
maintainability grounds.

## Options

* **Fork Ruff and add a plugin API**. Torpedoes Ruff's own argument for the
  refusal, and condemns us to rebase across every upstream merge — a
  permanent fork-maintenance tax.
* **Rewrite Pylint in Rust**. Months of work reimplementing a checker we
  would never match, while keeping none of its momentum.
* **New Rust linter that runs only custom WASM-plugin rules next to Ruff**.
  Ruff stays the binary it already is for the built-in rules, including
  `PL*`; ferrolint reuses Ruff's parser crates (`ruff_python_parser`,
  `ruff_python_ast`, `ruff_python_semantic`, pinned by git tag) and runs our
  own rules as WASM components.

## Decision

The third option. ferrolint scaffolds around a WASM plugin contract; Ruff
never sees a diff.

## Why WASM, not embedded Python or YAML patterns

* **Embedded Python** meant shipping a Python runtime that brings the GIL
  back, the startup cost we are trying to delete, and a host side channel
  for plugins we cannot sandbox as tightly.
* **YAML pattern DSLs** fall short the first time a rule needs scopes or
  bindings — queries the contract must answer from day one — and grow into
  a worse language every year they are kept.
* **WASM components** run in an empty WASI sandbox, per file under fuel and
  epoch limits, in parallel on every core, with the host controlling what a
  plugin can touch.

## Consequences

* Ruff parser crates are pinned to one git tag with a documented upgrade
  process; a bump is its own change with its own test.
* Type inference is phase 2: the plugin query API ships with AST and
  scope/binding queries first, and an inference API lands inside it later
  without breaking plugin ABI.
* Rule coverage grows write-a-plugin by write-a-plugin; nothing in ferrolint
  pretends to replace Ruff's catalog.
