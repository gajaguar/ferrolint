---
type: decision
title: cargo-deny
description: make check runs cargo-deny for advisories, licenses and sources; make install-rust installs the version pinned in mk/rust.mk with cargo install.
tags: [rust, security, toolchain]
status: stable
---

# cargo-deny

`make deny`, part of `make check`, runs `cargo deny --locked check` against
`deny.toml`: RustSec advisories, the license allow-list, banned crates and
duplicate versions, and the sources dependencies may come from. Only the
settings that differ from cargo-deny's default are in `deny.toml`: the
licenses allowed, and `deny` for an unknown registry or git source.

## Why cargo install

[The toolchain layering rule](../toolchain/layering-rule.md) gives mise the
tools that bootstrap an ecosystem and the ecosystem's package manager the
rest. Cargo has no manifest for development tools, so `make install-rust`
runs `cargo install --locked cargo-deny@$(CARGO_DENY_VERSION)`, with the
version pinned in `mk/rust.mk`: the one place that declares it. A cargo
install skips the build when that version is already present.

## Left out

`cargo-vet` (human audit records) costs more upkeep than a solo project
repays, and `cargo-audit` is a subset of `cargo-deny`'s advisory check.
