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
  doc comment lists what it leaves out, such as names used only in a type
  comment for `unused-variable`.
* `unused-import` counts a name the module's `__all__` lists as used, as the
  host reads that list (`modules.all-names`, the same reading as `W0614`);
  where the host cannot tell, the file's literal `__all__` lists stand.
* `undefined-variable` and `unused-import` ask the host for the names a
  `from m import *` binds (`modules.star-names`): the public names of a
  file found on the search path of the run (as `import-error` looks), beside
  the linted file for a relative `m`, a C module of the standard library the
  host has a table for, or one that star-imports them, ignoring the
  `__all__` of `m` as Pylint does. A name read that no star binds is
  reported, and an `import x` after a star that binds `x` is the star's, as
  in Pylint. A star of a module that `VIRTUAL_ENV` shows is nowhere (the
  `import-error` case) binds nothing, so the names read after it are
  reported, as in Pylint. A star the host cannot tell (a module with no
  `VIRTUAL_ENV` to search, or one that does not parse) keeps
  `undefined-variable` silent for the whole file. The answer depends on
  another file, so it is as stale as the
  [result cache](../requirements/result-cache.md) is.
* `undefined-variable` leaves a name that its scope or the module binds
  later (`print(x)` above `x = 1`) to Pylint's `E0601`, which ferrolint does
  not emit, and reads `del x` as a use of `x` before the function binds it.
* `bare-except` skips an `except:` whose body has a `raise` among its own
  statements, as Pylint's `_is_raising` does; a `raise` under an `if` still
  counts as bare.
* `redefined-builtin` checks a module and a function, once per name at its
  first binding, and reads `redefining-builtins-modules` and
  `allowed-redefined-builtins`. A function name the module also binds is
  left to `W0621`, as Pylint does. A parameter that the method of the first
  ancestor with that method repeats is excused (Pylint's
  `overridden_method`); the host reads that ancestor
  (`modules.overridden-parameters`) from the file, the project, the standard
  library and the installed packages, and where it cannot (a C class) the
  parameter is reported. It does not know the names a star import binds, so it
  misses a builtin that one binds, and it ignores `global __doc__`.
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

A release binary embeds these eleven plugins (ten with the `ty` feature,
which answers `E1102` itself and leaves out `not-callable`) and loads them
before any configured plugin, so every bundled rule is active with no `[[plugins]]`
table: `select` and `ignore` work on their codes and Pylint spellings
alone. `make release-build` and `make wheel` build the binary this way.
A binary from `cargo build`, such as the one the tests run, bundles none.

None of the eleven reads a plugin option or a comment range (`redefined-builtin`
reads two Pylint options, which every plugin receives). A plugin's own
`options` table in `[[plugins]]` and the comment query are for external
plugins (see [Configuration file](configuration-file.md) and
[Writing a plugin](writing-a-plugin.md)); a bundled plugin always gets an
empty table.

Errors name a bundled plugin as `<bundled>/<file>`. A `[[plugins]]` entry
that repeats one fails with the duplicate rule code error; `[[plugins]]`
is for external plugins, which still need `path` and `sha256`.
