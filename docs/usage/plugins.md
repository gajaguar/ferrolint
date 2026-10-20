---
type: reference
title: Bundled plugins
description: The Pylint checkers ported as plugins in this repository, with their codes, aliases and what they leave out.
tags: [plugins, rules]
status: stable
---

# Bundled plugins

`make plugins` builds each into
`target/wasm32-wasip2/release/ferrolint_plugin_<name>.wasm`. Every rule
answers to its Pylint msg-id and symbol as well as its code.

| Plugin                    | Code     | Pylint                            | Fix |
| :------------------------ | :------- | :-------------------------------- | :-- |
| `unused-import`           | `IMP001` | `W0611` `unused-import`           | yes |
| `undefined-variable`      | `NAM001` | `E0602` `undefined-variable`      | no  |
| `redefined-builtin`       | `NAM002` | `W0622` `redefined-builtin`       | no  |
| `unused-variable`         | `NAM003` | `W0612` `unused-variable`         | no  |
| `global-statement`        | `STM001` | `W0603` `global-statement`        | no  |
| `pointless-statement`     | `STM002` | `W0104` `pointless-statement`     | no  |
| `unnecessary-pass`        | `STM003` | `W0107` `unnecessary-pass`        | yes |
| `bare-except`             | `EXC001` | `W0702` `bare-except`             | no  |
| `singleton-comparison`    | `CMP001` | `C0121` `singleton-comparison`    | yes |
| `self-assigning-variable` | `ASG001` | `W0127` `self-assigning-variable` | no  |
| `not-callable`            | `CAL001` | `E1102` `not-callable`            | no  |

* `unused-import` is the SDK's sample and matches Pylint 4.1.3 on its
  fixtures (acceptance test AC-10).
* The others follow their Pylint checker's main behavior; each crate's
  doc comment lists what it leaves out, such as loop variables for
  `unused-variable` or every name of a module with a star import for
  `undefined-variable`.
* `singleton-comparison` quotes the operands as Pylint does in the simple
  cases (double-quoted strings print single-quoted, the operands keep their
  order, `bool(...)` appears outside a test condition) and is compared with
  Pylint on `plugins/singleton-comparison/fixtures`. Any other spelling
  (a string prefix, an escape, a line break, odd spacing, redundant
  parentheses) prints as written.
* `not-callable` uses the type inference query: it asks the host's type
  inference and reports only calls of a resolved builtin instance such
  as an `int` or a `list`. It is not one of the ten plugins the
  benchmark loads.

## Bundled by default

A release binary embeds these eleven plugins and loads them before any
configured plugin, so every bundled rule is active with no `[[plugins]]`
table: `select` and `ignore` work on their codes and Pylint spellings
alone. `make release-build` and `make wheel` build the binary this way.
A binary from `cargo build`, such as the one the tests run, bundles none.

None of the eleven reads an option or a comment range. A plugin's own
`options` table in `[[plugins]]` and the comment query are for external
plugins (see [Configuration file](configuration-file.md) and
[Writing a plugin](writing-a-plugin.md)); a bundled plugin always gets an
empty table.

Errors name a bundled plugin as `<bundled>/<file>`. A `[[plugins]]` entry
that repeats one fails with the duplicate rule code error; `[[plugins]]`
is for external plugins, which still need `path` and `sha256`.
