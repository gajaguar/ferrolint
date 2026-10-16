---
type: decision
title: Plugin sandbox environment
description: How the host meets the empty-WASI requirement - WASI linked but emptied, frozen clocks, fixed randomness, exit disabled - and how caps and instances work per file and per thread.
tags: [plugins, wasi]
status: stable
---

# Plugin sandbox environment

The [sandboxing requirement](../requirements/plugin-sandboxing.md) asks for
an empty WASI environment and refuses imports beyond the contract. A plugin
built with Rust's standard library for `wasm32-wasip2` imports WASI
interfaces even when it never touches a file (`wasi:cli/environment`,
`wasi:io/streams`, `wasi:random` for hash-map seeds), so refusing every WASI
import would refuse every Rust plugin.

## Decision

The host links WASI preview 2 and empties it:

* No preopened directory, no environment variable, no argument, and every
  socket kind denied: a file or socket open fails inside the instance.
* Stdin is closed and stdout and stderr are discarded.
* The wall and monotonic clocks are frozen at zero and both random sources
  return a fixed byte stream, so a plugin learns nothing about the machine
  and its output depends on its input alone, which the
  [determinism requirement](../requirements/output-determinism.md) needs.
* `exit` traps the call instead of ending the process.
* An import that is neither WASI preview 2 nor the contract fails to link,
  and the run stops with exit code 2.

## Caps and instances

* Each plugin compiles once per run. Every worker thread instantiates it
  once and reuses that instance file after file; a trap discards the
  instance and the next file gets a fresh one.
* Every check call gets a fresh budget: 2 billion units of fuel, a
  ten-second epoch deadline and 512 MiB of linear memory. The plugin
  cannot change them.
* A trap, a cap or a diagnostic carrying a code the plugin did not declare
  aborts that plugin on that file only. The file reports an `FL001`
  finding naming the plugin and the reason, the other files still lint,
  and the run exits with code 2 after printing its report, because its
  verdict is incomplete.
