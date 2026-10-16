# Rust

* [Compile target source](compile-target-source.md) - why `mise.toml` pins
  the toolchain, its components and its compile targets, and
  `rust-toolchain.toml` is absent.
* [Cargo.toml defaults](cargo-toml-defaults.md) - which settings are left out
  of `Cargo.toml` because they equal cargo's default.
* [Clippy lints](clippy-lints.md) - why `Cargo.toml` turns on pedantic and four
  restriction lints and forbids `unsafe`.
* [cargo-deny](cargo-deny.md) - what `make deny` checks, and why it installs
  with `cargo install` and not mise.
