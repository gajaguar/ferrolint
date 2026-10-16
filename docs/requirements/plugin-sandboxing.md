---
type: rule
title: Plugin sandboxing
description: What a WASM plugin may and may not touch - an empty WASI import set, imports denied beyond the contract, and per-file fuel and epoch budgets.
tags: [requirements, plugins]
status: stable
---

# Plugin sandboxing

`NFR-03`: a plugin is code a user installed by path into their
configuration, so the host treats it as untrusted input.

* The plugin's WASI environment is empty: no preopened directories, no
  environment variables, no host filesystem or network access. A plugin
  that tries to open a file or a socket fails the call because the
  import is not provided.
* Imports beyond the contract's own set are refused at instantiation:
  no clock, no randomness, no stdio inherited from the host process.
  A component that imports one fails to load.
* Each plugin runs under a per-file fuel budget (bounded instruction
  count) and an epoch deadline (bounded wall time) per linted file; the
  host picks the cap values, so a plugin cannot negotiate them.
* exceeding either cap aborts exactly that plugin's work on that file:
  the run drops its in-flight diagnostics for the file, and remaining
  files still lint. A plugin can therefore never hang a run, and
  plugins stay isolated from each other.
* The sandbox is not a shape check the host skips under load: the empty
  environment and the two caps hold on every file of every run, warm or
  cold.

A run in which a plugin hit a cap reports the termination (see
[Output formats](output.md)): the diagnostics carry it, and it does not
turn into a silent partial result.
