---
type: decision
title: cargo-deny
description: make check runs cargo-deny for advisories, licenses and sources; make install-rust installs the version pinned in mk/rust.mk with cargo install.
tags: [rust, security, toolchain]
status: stable
---

# cargo-deny

`make deny`, part of `make check`, runs `cargo deny --locked check -A advisory-not-detected`
against `deny.toml`: RustSec advisories, the license allow-list, banned crates and
duplicate versions, and the sources dependencies may come from. Only the
settings that differ from cargo-deny's default are in `deny.toml`: the
licenses allowed, and `deny` for an unknown registry or git source.

## The `ty` feature

`make deny` checks the default graph. The optional `ty` feature brings crates
that graph never meets, so `make ty-check` runs `cargo deny --features ty`
too. Four crates get a license exception by name (`colored`, `notify`,
`option-ext`, `version-ranges`: MPL-2.0 and CC0-1.0), not a global allowance.
Six advisories on `zip`, `zstd` and `zstd-safe` are ignored with a reason in
`deny.toml`: they have no patched release, and ty reads only its embedded
typeshed archive, never extracting one. Without the feature those entries match
no crate, so `make deny` passes `-A advisory-not-detected` to keep the
unused-entry warning out of its output; `deny.toml` has no key for that.

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
