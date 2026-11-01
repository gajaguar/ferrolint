# ferrolint

[![rust][ci-badge]][ci] [![PyPI][pypi-badge]][pypi]
[![crates.io][crate-badge]][crate] [![License: MIT][license-badge]][license]

Extensible Python linter in Rust that runs custom rules, loaded as
[WASM](https://webassembly.org/) plugins, next to
[Ruff](https://docs.astral.sh/ruff/).

## Table of contents

- [About](#about)
- [Key features](#key-features)
- [Requirements](#requirements)
- [Usage](#usage)
- [Getting started](#getting-started)
- [How a run works](#how-a-run-works)
- [Writing a plugin](#writing-a-plugin)
- [Type inference with ty](#type-inference-with-ty)
- [Security](#security)
- [Contributing](#contributing)
- [License](#license)

## About

ferrolint lints Python code with Ruff's own parser crates
(`ruff_python_parser`, `ruff_python_ast`, `ruff_python_semantic`) and runs
checks that ship as WASM plugins. Instead of a plugin host interpreting
Python, each check is a compiled WebAssembly component: the contract feeds it
AST visitor events and lets it ask scope and binding questions, and the plugin
answers with diagnostics carrying a code, message, range, severity and an
optional fix.

Ruff stays in the loop and keeps every built-in rule, including the Pylint
`PL*` checks. ferrolint is the layer for checks Ruff does not have: a team's
custom rules, ported Pylint checkers, and Pylint message-ID/symbol mappings,
all running in the same pass.

## Key features

- **Plugins as WebAssembly components.** Each plugin runs in an empty sandbox
  with per-file fuel and time caps, and loads only with its pinned SHA-256.
- **Pylint compatibility.** Rules keep Pylint's message IDs and symbols, so
  selecting, ignoring and `# pylint: disable=` comments keep working.
- **Speed on large codebases.** On a million lines with the ten benchmark
  plugins, a cold run takes about 6.5 s on four cores and a warm run 0.25 s;
  Pylint running the same ten checks takes 167 s. `make bench` measures it.
- **Incremental runs.** A content-keyed cache in `.ferrolint-cache/` and
  `--diff-base` to lint only what changed.
- **Deterministic output** in text, JSON, SARIF or JUnit.
- **One static binary** for Linux x86_64 and aarch64 and for macOS aarch64.

## Requirements

- Linux (x86_64 or aarch64) or macOS (aarch64). There is no Windows build.
- Python 3.9 or later and `pip` or `uv`, to install the wheel. The linter
  itself is a native binary and lints any Python code.
- To write a plugin: Rust and the `wasm32-wasip2` target
  (`rustup target add wasm32-wasip2`).

## Usage

Lint the current directory:

```bash
ferrolint
```

Lint chosen paths with an explicit configuration file, in SARIF:

```bash
ferrolint --config ci.toml --format sarif src tests
```

| Option              | Default | Effect                                               |
| :------------------ | :------ | :--------------------------------------------------- |
| `[PATHS]...`        | `.`     | Files or directories to lint.                        |
| `-c`, `--config`    | none    | A `ferrolint.toml` or `pyproject.toml` to read.      |
| `--threads <N>`     | `0`     | Worker threads; `0` uses every core.                 |
| `--format <FORMAT>` | `text`  | `text`, `json`, `sarif` or `junit`.                  |
| `--diff-base <REF>` | none    | Lint only files changed since the merge base of REF. |
| `--no-cache`        | off     | Neither read nor write `.ferrolint-cache/`.          |
| `--stats`           | off     | Print files, cache hits, parses and plugin runs.     |

| Exit code | Meaning                               |
| :-------- | :------------------------------------ |
| `0`       | No findings.                          |
| `1`       | At least one finding.                 |
| `2`       | Usage, configuration or plugin error. |

Configuration lives in `ferrolint.toml` or in the `[tool.ferrolint]` table of
`pyproject.toml` ([all keys][config]):

```toml
select = ["FL002"]
ignore = ["FL003"]
exclude = ["vendor/"]

[[plugins]]
path = "plugins/my_checker.wasm"
sha256 = "…"

[per-file-ignores]
"tests/*.py" = ["FL002"]
```

## Getting started

Install the wheel from PyPI:

```bash
pip install ferrolint        # or: uv tool install ferrolint
ferrolint --version
```

To try an unreleased commit, install from Git. The build needs the pinned
Rust toolchain and the `wasm32-wasip2` target, and the first build compiles
Wasmtime, which takes minutes:

```bash
pip install "git+https://github.com/gajaguar/ferrolint"
```

## How a run works

A run discovers the Python files under its targets, honouring `.gitignore` and
the configured excludes, parses them in parallel with Ruff's parser, and runs
the host's own rules plus every configured WASM plugin against the
[`plugin.wit`][wit] contract. The rules and their Pylint counterparts are
listed in [Native rules][native-rules]. The requirements and the design
decisions live in the `docs/` bundle: [requirements][requirements] and
[decisions][decisions].

## Writing a plugin

A plugin is a Rust `cdylib` built to `wasm32-wasip2` against
[`ferrolint-plugin-sdk`][crate]:

```toml
[dependencies]
ferrolint-plugin-sdk = "1"
```

The SDK's version is the version of the plugin contract: a plugin built
against any 1.x loads in a ferrolint 1.x host. The sample
[`unused-import`][sample] ports Pylint's `W0611` and matches its findings on
the fixtures. [Writing a plugin][writing] builds it step by step, and
[A plugin outside this repository][out-of-tree] covers a plugin kept in its
own crate.

## Type inference with ty

[ty](https://docs.astral.sh/ty/) is an optional extension that adds ten
Pylint rules needing the type of a value. The published wheels are built
without it. To use it, build with `cargo build --release --features ty` and set
`VIRTUAL_ENV` to the project's environment. See [Rules answered by
ty][ty-rules].

## Security

Report a vulnerability by emailing <dev@gajaguar.com> rather than opening a
public issue. [SECURITY.md][security] has the supported versions.

## Contributing

See [CONTRIBUTING.md][contributing].

## License

ferrolint is released under the MIT License; see [LICENSE][license].

<!-- Absolute URLs: PyPI renders this file away from the repository. -->

[ci]: https://github.com/gajaguar/ferrolint/actions/workflows/rust.yml
[ci-badge]: https://img.shields.io/github/actions/workflow/status/gajaguar/ferrolint/rust.yml?branch=main&style=flat-square&label=rust
[pypi]: https://pypi.org/project/ferrolint/
[pypi-badge]: https://img.shields.io/pypi/v/ferrolint?style=flat-square
[crate]: https://crates.io/crates/ferrolint-plugin-sdk
[crate-badge]: https://img.shields.io/crates/v/ferrolint-plugin-sdk?style=flat-square&label=sdk
[license]: https://github.com/gajaguar/ferrolint/blob/main/LICENSE
[license-badge]: https://img.shields.io/github/license/gajaguar/ferrolint?style=flat-square
[contributing]: https://github.com/gajaguar/ferrolint/blob/main/CONTRIBUTING.md
[security]: https://github.com/gajaguar/ferrolint/blob/main/SECURITY.md
[wit]: https://github.com/gajaguar/ferrolint/blob/main/crates/ferrolint-plugin-sdk/wit/plugin.wit
[sample]: https://github.com/gajaguar/ferrolint/blob/main/plugins/unused-import/src/lib.rs
[config]: https://github.com/gajaguar/ferrolint/blob/main/docs/usage/configuration-file.md
[native-rules]: https://github.com/gajaguar/ferrolint/blob/main/docs/usage/native-rules.md
[requirements]: https://github.com/gajaguar/ferrolint/blob/main/docs/requirements/index.md
[decisions]: https://github.com/gajaguar/ferrolint/tree/main/docs/decisions
[writing]: https://github.com/gajaguar/ferrolint/blob/main/docs/usage/writing-a-plugin.md
[out-of-tree]: https://github.com/gajaguar/ferrolint/blob/main/docs/usage/out-of-tree-plugin.md
[ty-rules]: https://github.com/gajaguar/ferrolint/blob/main/docs/usage/ty-rules.md
