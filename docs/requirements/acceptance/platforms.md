---
type: rule
title: Acceptance criteria - platforms
description: Given/When/Then criteria for the release targets, the pinned Ruff crate set and the quality gate, mapped to NFR-06, NFR-07 and NFR-08.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: platforms

## AC-32: One static binary per target (NFR-06)

* Given the three release targets - linux x86_64, linux aarch64, macos
  aarch64 -
* then a release build produces one self-contained binary per target
  that starts (run, help, config validation and the [start-up
  budget](performance.md)) with no system package, interpreter or
  runtime download on the target, to the smallest smoke check.

## AC-33: Ruff crates move only one-way (NFR-07)

* Given the three re-used Ruff parser crates,
* then they are pinned in `Cargo.toml` to one git tag, `cargo tree`
  resolves every use through that single pin, and the documented
  upgrade procedure names the tag change as one commit touching the
  pins plus the fixtures.

## AC-34: The gate is green over the whole workspace (NFR-08)

* Given `make check` and `make test` in a checkout whose crates are
  byte-for-byte identical to a release's workspace, with clippy
  pedantic at `-D warnings` and `cargo-deny` checking advisories,
  licenses and sources,
* then both pass on every plugin, on the SDK crate and on the linter
  itself, so a release ships through the same commands the agent and CI
  run.
