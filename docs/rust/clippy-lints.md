---
type: decision
title: Clippy lints
description: Cargo.toml turns on Clippy's pedantic group and four restriction lints, and forbids unsafe code; make clippy denies the warnings.
tags: [rust, lint]
status: stable
---

# Clippy lints

The lint levels live in `[lints]` in `Cargo.toml`, not in flags on the
`cargo clippy` command line: cargo, rust-analyzer and editors read the table,
so an editor shows what `make check` enforces. `make clippy` keeps
`-D warnings`, so a `warn` blocks the commit without breaking the build while
iterating.

| Setting                               | Why                                                      |
| :------------------------------------ | :------------------------------------------------------- |
| `unsafe_code = "forbid"`              | an agent-written crate has no business with `unsafe`     |
| `clippy::pedantic` (priority -1)      | strict lints that catch real mistakes; each `allow` wins |
| `unwrap_used`, `expect_used`          | return an error instead of panicking                     |
| `dbg_macro`, `todo`                   | leftover scaffolding never reaches a commit              |
| `must_use_candidate`, `missing_*_doc` | allowed: pedantic noise that adds no safety              |

`clippy.toml` allows `unwrap` and `expect` inside tests, where a panic is the
assertion. The whole `restriction` group stays off: its lints conflict with
each other and are meant to be picked one at a time.

A crate that needs `unsafe` drops the `forbid` and says why in `Cargo.toml`.
