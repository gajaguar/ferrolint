---
type: reference
title: Release builds
description: How a release is built and checked for its three targets, its binaries and PyPI wheels, and how to reproduce one locally.
tags: [release, toolchain]
status: stable
---

# Release builds

Pushing a `v*` tag (or starting the `release` workflow by hand) builds one
binary per target on a runner of that architecture:

| Target                       | Runner             | Linking                           |
| :--------------------------- | :----------------- | :-------------------------------- |
| `x86_64-unknown-linux-musl`  | `ubuntu-latest`    | static (musl)                     |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | static (musl)                     |
| `aarch64-apple-darwin`       | `macos-latest`     | system libraries only (libSystem) |

* The Linux jobs install `musl-tools`: the parser's stack-growth
  dependency compiles a little C.
* Each job runs `make release-build TARGET=...` and then
  `make smoke BIN=...`, which runs the binary in an empty directory:
  `--version`, `--help`, a malformed and an unknown-rule configuration
  (exit 2), a clean file (exit 0), a finding (exit 1), and the 100 ms
  budget to the first parse. The Linux jobs also check that `file` calls
  the binary static.
* The binaries are uploaded as workflow artifacts. Each release binary
  embeds the eleven bundled plugins (`make release-build` sets
  `FERROLINT_PLUGINS` to the `make plugins` output), so every bundled rule
  is active by default. Tagging stays a manual step, as the
  [versioning rules](../conventions/versioning.md) require.

## Wheels

`make wheel TARGET=...` builds the same binary into a PyPI wheel under
`target/wheels/`, with maturin (`bindings = "bin"`, from
`crates/ferrolint/pyproject.toml`). Each wheel job installs its wheel in a
fresh venv, runs `ferrolint --version` and `make smoke` on the installed
command, and uploads a `wheel-<target>` artifact. There is no sdist and no
Windows wheel.

* The Linux musl binaries are static, so their wheels are retagged
  `manylinux_2_17_<arch>.musllinux_1_2_<arch>`; pip on glibc then accepts
  them. The macOS wheel keeps its `macosx_11_0_arm64` tag.

## Installing from source

`pip install "git+ssh://git@github.com/<owner>/ferrolint#subdirectory=crates/ferrolint"`
(or a Poetry git dependency with `subdirectory = "crates/ferrolint"`) builds
with maturin and the `bundle` cargo feature, which `pyproject.toml` enables.

* The feature makes `build.rs` build the repository plugins for
  `wasm32-wasip2` into the build's own target directory and bundle them, as
  `make wheel` does with prebuilt ones. `FERROLINT_PLUGINS` still takes
  priority.
* The machine needs the pinned Rust toolchain and the `wasm32-wasip2` target;
  the first build compiles Wasmtime and takes minutes.
* A plain `cargo build` has no `bundle` feature and an empty bundle.

## Publishing to PyPI

The `publish` job runs only when a GitHub Release is published. It
downloads the `wheel-*` artifacts and uploads them with
`pypa/gh-action-pypi-publish` through PyPI Trusted Publishing, in the
`pypi` environment. Publishing the Release is therefore the step that
ships the wheels; a tag push alone does not.

One-time setup, before the first publish:

* On PyPI, add a pending trusted publisher for project `ferrolint`, owner
  `gajaguar`, repository `ferrolint`, workflow `release.yml` and
  environment `pypi`.
* On GitHub, create the `pypi` environment for the repository.

The targets are pinned in `mise.toml` with the toolchain, so
`make release-build TARGET=x86_64-unknown-linux-musl` works locally once
`musl-tools` is installed.
