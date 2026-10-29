---
type: reference
title: Release builds
description: How a release is built and checked for its three targets, its binaries, PyPI wheels and sdist, and how to reproduce one locally.
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
command (and `python -m ferrolint`), and uploads a `wheel-<target>` artifact.
There is no Windows wheel.

* The root `pyproject.toml` points maturin at `crates/ferrolint/Cargo.toml`;
  `python/ferrolint/` holds the `python -m ferrolint` launcher, which runs
  the installed binary.
* `make sdist` builds the source distribution, and the `sdist` job installs
  it in a fresh venv and runs `make smoke`. maturin cuts the sdist's
  workspace down to `crates/*`, so `build.rs` lists `plugins/*` again before
  it builds the plugins.

* The Linux musl binaries are static, so their wheels are retagged
  `manylinux_2_17_<arch>.musllinux_1_2_<arch>`; pip on glibc then accepts
  them. The macOS wheel keeps its `macosx_11_0_arm64` tag.

## The wheel has no ty

The wheels and the sdist are built without the `ty` cargo feature, so the ten
[rules answered by ty](../usage/ty-rules.md) are absent from them. Measured on
`aarch64-apple-darwin` with five jobs:

| Build                  | Binary  | Wheel   | Cold build | Compiler peak memory |
| :--------------------- | ------: | ------: | ---------: | -------------------: |
| default                | 32.3 MB |  8.9 MB |      127 s |              1.46 GB |
| `--features ty`        | 60.6 MB | 17.5 MB |      196 s |              2.80 GB |

Size, build time and `make smoke` (1.2 ms to the first parse) all pass. The
wheel was left without `ty` because, without `VIRTUAL_ENV`, ty could not read
the installed packages and the ten rules reported 29 findings on one Django
codebase that neither the run with the environment nor Pylint give (24
`E1133`, 5 `E1135`), 10% on top of the 275 it reports with it. Since 0.35.3
`ferrolint` does not start ty without the variable: it warns and the rules
report nothing (0 findings on that codebase), so that reason no longer holds.
Whether the wheels carry `ty` is a decision still to be taken.

To build with it, run `make release-build TARGET=... FEATURES=ty`, or
`cargo build --release --features ty`, and set `VIRTUAL_ENV` when you run it.

## Installing from source

`pip install "git+ssh://git@github.com/<owner>/ferrolint"` (or a Poetry git
dependency) builds with maturin and the `bundle` cargo feature, which
`pyproject.toml` enables.

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
ships the wheels; a tag push alone does not. Nothing ships before 1.0.0: the
job skips a `v0.*` tag, so remove that test, and do the one-time setup
below, when 1.0.0 is released.

One-time setup, before the first publish:

* On PyPI, add a pending trusted publisher for project `ferrolint`, owner
  `gajaguar`, repository `ferrolint`, workflow `release.yml` and
  environment `pypi`.
* On GitHub, create the `pypi` environment for the repository.

The targets are pinned in `mise.toml` with the toolchain, so
`make release-build TARGET=x86_64-unknown-linux-musl` works locally once
`musl-tools` is installed.
