---
type: guide
title: A plugin outside this repository
description: How a plugin crate in its own repository depends on the SDK by Git revision, builds to wasm32-wasip2 and loads into ferrolint.
tags: [plugins]
status: stable
---

# A plugin outside this repository

[Writing a plugin](writing-a-plugin.md) builds the sample inside this
workspace. A plugin kept in another repository needs three things different.

## Depend on the SDK by revision

```toml
[lib]
crate-type = ["cdylib"]

[dependencies.ferrolint-plugin-sdk]
git = "ssh://git@github.com/gajaguar/ferrolint.git"
rev = "<commit>"
```

Pin a `rev`, never a branch: the SDK's version is the version of the
[`wit/plugin.wit`](../../wit/plugin.wit) contract, and a plugin built against
one revision loads only in a host that speaks the same contract. The SDK
reads `../../wit` relative to its own crate, which resolves inside the Git
checkout, so no change to this repository is needed.

If Cargo's built-in Git client cannot authenticate over SSH, let it call the
`git` command: put `net.git-fetch-with-cli = true` in the plugin repository's
`.cargo/config.toml`.

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
