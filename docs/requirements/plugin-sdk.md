---
type: rule
title: Plugin SDK and sample plugin
description: The Rust SDK crate that wraps the WIT contract, and the sample plugin porting one real Pylint checker.
tags: [requirements, plugins]
status: stable
---

# Plugin SDK and sample plugin

`FR-04`: plugin authors work in Rust against a small SDK crate, and one
real Pylint checker exists end to end as proof the contract works.

* The SDK crate (`ferrolint-plugin-sdk`) compiles a Rust checker to a
  `wasm32-wasip2` component that implements the
  [plugin contract](plugin-contract.md): it turns the raw WIT exports
  into a visitor with default no-op methods, a diagnostics builder, and
  typed constructors for scopes, bindings and fixes.
* The SDK exposes no host crate types beyond what the contract needs, so
  a plugin crate does not link the linter itself.
* The SDK's `Context::option(key)` returns a configuration option as the
  plugin received it in the file metadata, or none when it is unset; a
  checker parses the text itself.
* A sample plugin in the workspace ports one designated real Pylint
  checker, chosen as the smallest checker that needs queries (not just
  AST shape) - it exercises events, scope and binding queries and one
  diagnostic with fix data.
* The sample runs against its checker's own Pylint output on fixtures:
  its findings match the checker's msg-id-equivalent codes, and a decode
  of the fixture set blocks a regression in `make test`.
* The SDK's version matches the WIT interface version it wraps, and the
  sample pins both - a plugin cannot compile against a contract it does
  not implement.

Versioning the pair (SDK, contract) together is the upgrade path: bump
them in the same change, update the sample, and regenerate.
