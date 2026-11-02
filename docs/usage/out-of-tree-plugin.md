---
type: guide
title: A plugin outside this repository
description: How a plugin crate in its own repository depends on the SDK from crates.io, builds to wasm32-wasip2 and loads into ferrolint.
tags: [plugins]
status: stable
---

# A plugin outside this repository

[Writing a plugin](writing-a-plugin.md) builds the sample inside this
workspace. A plugin kept in another repository needs three things different.

## Depend on the SDK from crates.io

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
ferrolint-plugin-sdk = "1"
```

The SDK's version is the version of the
[`plugin.wit`](../../crates/ferrolint-plugin-sdk/wit/plugin.wit) contract. A
plugin built against 1.1 or later loads in a ferrolint 1.x host of the same or a
later minor version; a build against 0.x or 1.0 is refused. The contract file ships
inside the crate, so nothing else is needed.

To try an unreleased SDK, depend on it by Git revision (`git =` and `rev =`,
never a branch) instead.

## Build

```sh
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
```

The output is `target/wasm32-wasip2/release/<crate_name>.wasm`, with dashes in
the crate name turned into underscores. The plugin needs no workspace of this
repository, and one crate can hold many rules.

## Load

Load the file with its SHA-256, as for any external plugin:

```toml
[[plugins]]
path = "path/to/plugin_crate.wasm"
sha256 = "…"   # sha256sum path/to/plugin_crate.wasm
```

Give each rule its Pylint msg-id and symbol as aliases, so that selecting,
ignoring and `# pylint: disable=` comments keep working with the old names.
