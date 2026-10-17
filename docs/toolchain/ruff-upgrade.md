---
type: playbook
title: Ruff upgrade
description: How to move the pinned Ruff crates to a new tag in one change, and the tag the workspace is on.
tags: [ruff, toolchain]
status: stable
---

# Ruff upgrade

The workspace pins every Ruff crate it uses (`ruff_python_parser`,
`ruff_python_ast`, `ruff_python_semantic`, `ruff_python_stdlib`,
`ruff_text_size`) to one git tag in `Cargo.toml`. Acceptance test AC-33
fails when two tags appear or `Cargo.lock` resolves a Ruff package
through anything else.

Current tag: `0.17.1`.

## Procedure

1. Change the `tag` of every `ruff_*` entry in `[workspace.dependencies]`
   to the new tag, all in the same edit.
2. Run `cargo update -p ruff_python_parser` so `Cargo.lock` follows; every
   Ruff package moves with it.
3. Fix what the new tag breaks, usually in `crates/ferrolint/src/ast.rs`
   (node kinds) and `semantic.rs` (new statement or expression shapes).
4. Run `make test`. The sample plugin's fixtures (AC-10) and the semantic
   unit tests are the parity set: a change in what the parser produces
   shows up there.
5. Update the current tag above, and make the pins, the lock file, the
   fixes and this note one commit.
