---
type: rule
title: Platforms and release gate
description: What a release ships - one static binary per target platform, against one pinned Ruff crate set with a documented upgrade path, and the quality gate it must pass.
tags: [requirements, platforms]
status: stable
---

# Platforms and release gate

`NFR-06`, `NFR-07` and `NFR-08` fix what each release must be.

## Platforms

* Releases build one self-contained static binary (`NFR-06`) for the
  three supported targets: Linux x86_64, Linux aarch64 and macOS
  aarch64. The binary embeds everything the run needs - the WASM runtime
  included - and needs no system package or interpreter at runtime.
* Releases also ship a PyPI wheel per platform that contains the binary
  (see [Release builds](../toolchain/release-builds.md)).
* Cross-compilation and CI build the same set; every release commit
  passes targets with the pinning in `mise.toml` (see
  [Compile target source](../rust/compile-target-source.md)).

## Pinned Ruff crates

* The Ruff parser crates the linter re-uses - `ruff_python_parser`,
  `ruff_python_ast`, `ruff_python_semantic` - are pinned to one git tag,
  in `Cargo.toml`: either none of them moves, or all of them do
  (`NFR-07`).
* The upgrade path is documented and mechanical: bump the tag, fix
  breakage the tag introduces, re-run the fixture parity set built on
  the sample plugin, and record the tag in this bundle's toolchain
  notes - so a future bump starts from one place, not from archaeology.

## Quality gate

* `make check` and `make test` pass before any commit (`NFR-08`),
  including Clippy with the pedantic group and restriction set at
  `-D warnings` (see [Clippy lints](../rust/clippy-lints.md)) and
  `cargo-deny` for advisories, licenses and sources (see
  [cargo-deny](../rust/cargo-deny.md)).
* The gate holds for the full workspace: plugins, SDK, and linter crate
  together - a crate that fails it alone holds a release.
