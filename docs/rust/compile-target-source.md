---
type: decision
title: Compile target source
description: mise.toml pins the Rust toolchain, its components and its extra compile targets; rust-toolchain.toml is absent because mise ignores it.
tags: [rust, toolchain]
status: stable
---

# Compile target source

`mise.toml` is the single source for the pinned Rust toolchain and for the
components and compile targets beyond the defaults:

```toml
[tools.rust]
version = "1.99"
components = ["clippy", "rustfmt"]
targets = ["wasm32-wasip2"]
```

`rust-toolchain.toml` is intentionally absent. mise's `rust` backend installs
the version it is given and applies the `components` and `targets` of that
inline table; it does not read `rust-toolchain.toml`, so a target named there
is missing from the toolchain until someone runs `rustup target add` by hand.
Keeping the one declaration in `mise.toml` is what makes `make install` and
CI install the same toolchain.

A crate that builds for no target beyond the host should drop the `targets`
entry rather than carry a download it never uses.
