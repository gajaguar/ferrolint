# ferrolint-plugin-sdk

Guest-side SDK for writing [ferrolint](https://github.com/gajaguar/ferrolint)
plugins: a plugin is a Rust `cdylib` compiled to a `wasm32-wasip2` component
that implements the `ferrolint:plugin` contract.

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
ferrolint-plugin-sdk = "1"
```

```sh
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
```

The SDK's version is the version of the contract: a plugin built against
1.x loads in any ferrolint 1.x host. See
[Writing a plugin](https://github.com/gajaguar/ferrolint/blob/main/docs/usage/writing-a-plugin.md).
