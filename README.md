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

Scaffold only: nothing is implemented yet. The repository pins the
conventions and the plan, and the linter, the plugin contract and the plugin
SDK do not exist yet.

The requirements and the design decisions live in the `docs/` OKF bundle:
[docs/requirements/index.md](docs/requirements/index.md) and
[docs/decisions/](docs/decisions/). Those directories land slightly after
this README; until then, the plan is summarized in this repository's issues.

## Planned usage

Once implemented, ferrolint will take files or directories as arguments,
honoring `.gitignore` and exclude configuration, and read its options from
`ferrolint.toml` or the `[tool.ferrolint]` table of `pyproject.toml`. A
configuration names the plugins with a path and a SHA-256 hash, and selects
or ignores rule codes, including per-file-ignores:

```toml
# ferrolint.toml — planned, not final
[plugins.my_checker]
path = "plugins/my_checker.wasm"
sha256 = "…"

select = ["W1"]
ignore = ["W2"]
per-file-ignores = { "tests/**" = ["W1"] }
```

Inline suppressions are planned as `# ferrolint:disable=CODE`, while staying
compatible with existing `# pylint:disable=CODE` and `# noqa: CODE` comments.
Exit codes follow a simple contract: `0` when clean, `1` when findings exist,
`2` for usage, configuration or plugin errors.

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
