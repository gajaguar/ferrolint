---
type: reference
title: Release builds
description: How a release is built and checked for its three targets, and how to reproduce one locally.
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
* The binaries are uploaded as workflow artifacts; publishing a GitHub
  Release from them stays a manual step, as the
  [versioning rules](../conventions/versioning.md) require.

The targets are pinned in `mise.toml` with the toolchain, so
`make release-build TARGET=x86_64-unknown-linux-musl` works locally once
`musl-tools` is installed.
