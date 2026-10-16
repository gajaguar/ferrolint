---
type: rule
title: Cargo.toml defaults
description: Cargo.toml holds only settings that change cargo's behavior; anything equal to the default is left out.
tags: [rust]
status: stable
---

# Cargo.toml defaults

`Cargo.toml` holds only settings that change cargo's behavior. A setting
equal to cargo's default is noise: it hides the real decisions and drifts
when the default moves. This is the same rule the Python layer follows for
`pyproject.toml`.

These are left out on purpose:

| Omitted setting        | Why it is not needed                                                  |
| :--------------------- | :-------------------------------------------------------------------- |
| `[lib] path`           | cargo discovers `src/lib.rs` by itself                                |
| `[lib] crate-type`     | the default is a `lib`; a `cdylib` — a WASM plugin — states otherwise |
| `readme`               | cargo picks up `README.md` by name                                    |
| `license-file`         | cargo picks up `LICENSE` by name                                      |
| `authors`              | cargo no longer reads it, and `LICENSE` names the holder              |
| `rust-version`         | the toolchain is pinned in `mise.toml` — see below                    |
| `[dependencies]`       | an empty table says nothing its absence does not                      |
| `[profile.*]` settings | every knob equals its default until the crate needs the exception     |

`rust-version` is the one that a published crate usually adds. A scaffold
leaves it out because `mise.toml` already pins the toolchain, and a second
place to change the version is a second place to be wrong — see
[Compile target source](compile-target-source.md).
