# ferrolint

Extensible Python linter in Rust that runs only custom rules, loaded as
[WASM](https://webassembly.org/) plugins alongside
[Ruff](https://docs.astral.sh/ruff/).

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

It is built for speed on huge codebases: parallel execution over many cores,
compilation of the plugins once per run, a content-keyed cache, deterministic
output in text, JSON, SARIF or JUnit, and a single static binary for Linux
x86_64 and aarch64 plus macOS aarch64.

## Status

In progress, epic by epic (see [docs/backlog.md](docs/backlog.md)). A run
discovers the Python files under its targets, honouring `.gitignore` and the
configured excludes, parses them in parallel with Ruff's parser, and runs
the host's own rules plus every configured WASM plugin against the
[`wit/plugin.wit`](wit/plugin.wit) contract, each in an empty sandbox with
per-file fuel and time caps. Plugins are written in Rust against
`ferrolint-plugin-sdk`; the sample in
[`plugins/unused-import`](plugins/unused-import/src/lib.rs) ports Pylint's
`unused-import` (W0611) and matches its findings on the fixtures (see
[docs/usage/writing-a-plugin.md](docs/usage/writing-a-plugin.md)).

A run exits `0` when clean, `1` with findings and `2` on a usage,
configuration or plugin error.

The requirements and the design decisions live in the `docs/` OKF bundle:
[docs/requirements/index.md](docs/requirements/index.md) and
[docs/decisions/](docs/decisions/).

## Usage

```bash
ferrolint [PATHS]...            # default: the current directory
ferrolint --config ci.toml src  # an explicit configuration file
ferrolint --threads 4           # 0, the default, uses every core
ferrolint --format json         # text (default) or json
```

Configuration lives in `ferrolint.toml` or the `[tool.ferrolint]` table of
`pyproject.toml`; see
[docs/usage/configuration-file.md](docs/usage/configuration-file.md):

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

## Development

Run the project's wiring from `make` targets:

```bash
make install
make check
make test
```

Run `make help` for the full target list.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

See [LICENSE](LICENSE).
