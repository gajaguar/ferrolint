---
type: guide
title: Writing a plugin
description: How to write, build and load a ferrolint plugin with the Rust SDK, using the unused-import sample as the template.
tags: [plugins]
status: stable
---

# Writing a plugin

A plugin is a Rust crate built as a `cdylib` for `wasm32-wasip2` against
`ferrolint-plugin-sdk`. The sample in
[`plugins/unused-import`](../../plugins/unused-import/src/lib.rs), a port
of Pylint's `unused-import` (W0611), is the template.

## The crate

```toml
[lib]
crate-type = ["cdylib"]

[dependencies.ferrolint-plugin-sdk]
path = "../../crates/ferrolint-plugin-sdk"
version = "=0.4.3"
```

Pin the SDK exactly: its version is the version of the
[`plugin.wit`](../../crates/ferrolint-plugin-sdk/wit/plugin.wit) contract it
implements, and a test fails when the two drift. A contract change bumps both
in the same change and updates the sample with them.

## The checker

Implement `Checker` on a `Default` type and export it:

```rust
use ferrolint_plugin_sdk::prelude::*;

#[derive(Default)]
struct NoPrint;

impl Checker for NoPrint {
    fn info() -> Info {
        let rule = Rule::new("NP001", "print-call", "print() is called.");
        Info::new("no-print", "0.1.0").rule(rule.pylint("W9999", "print-call"))
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        let print = node.name == Some("print");
        if node.kind == NodeKind::Name && print {
            cx.report(Diagnostic::new("NP001", "print() is called", node.range));
        }
    }
}

export_checker!(NoPrint);
```

* Each file gets a fresh value. `start_file`, then `visit` and `leave` for
  every node in source order, then `end_file`; all default to doing
  nothing.
* A `Node` borrows its `name` and `detail` from the file's string table.
* `Context` holds the file (`path`, `module`, `source`), the node list with
  `parent`, `children`, `ancestors` and `text`, and the host's answers:
  `scope`, `visible_bindings` and `resolve`, plus `infer` for a
  best-effort type. Report on an inferred type only when its `resolved`
  is true; `plugins/not-callable` shows the pattern. `option("max-args")`
  gives a configuration option as text, or none when it is unset.
* `comments()` lists the file's comments, each with its `range` and
  `text` (the `#` included), and `plugin_option("markers")` reads an
  option set only for this plugin, in the `options` table of its
  `[[plugins]]` entry. `plugins/test-markers` uses both: it reports the
  comments that hold a marker its `markers` option names.
* A plugin built with SDK 0.4.3 needs a host of contract 0.4.3. That host
  still loads plugins built with SDK 0.4.0 to 0.4.2, but refuses 0.3.x or
  older: rebuild those.
* `star_names(alias)` gives the names the `from m import *` at an alias
  node binds, or none when the host cannot tell. `plugins/undefined-variable`
  uses it. `overridden_parameters(method)` gives the parameter names of the
  method that a `FunctionDef` overrides, or none; `plugins/redefined-builtin`
  uses it. `all_names()` gives the names the module's `__all__` makes Pylint
  take for used, or none; `plugins/unused-import` uses it.
* A `Comment` also has `own_line`, true when only whitespace precedes it on
  its line, so a plugin can tell a comment-only line from a trailing comment.
* A `Diagnostic` takes its rule's severity unless it sets one, and may
  carry a `Fix` (`Fix::replace`, `Fix::delete`). The host reports a fix and
  never applies it.
* A rule's `pylint` aliases let users select, ignore and suppress it by
  Pylint msg-id or symbol. Reporting a code the plugin did not declare
  aborts the plugin on that file.

## Build and load

`make plugins` builds every workspace plugin into
`target/wasm32-wasip2/release/`. Load one with its SHA-256:

```toml
[[plugins]]
path = "target/wasm32-wasip2/release/my_plugin.wasm"
sha256 = "…"   # sha256sum target/wasm32-wasip2/release/my_plugin.wasm
options = { markers = "TODO" }   # optional: read with plugin_option
```

## Parity with Pylint

The sample's fixtures sit next to Pylint's own findings on them, recorded
by `make sample-fixtures` with the Pylint version pinned in `mk/rust.mk`
and checked by acceptance test AC-10. Record a new fixture's findings the
same way instead of writing them by hand.
