# Contributing

Thanks for considering a contribution. Bug reports, feature ideas,
documentation fixes, code, and plugins are all welcome.

The requirements live under [`docs/requirements/`](docs/requirements/) and the
decisions under [`docs/decisions/`](docs/decisions/).

## Ways to contribute

- Report a bug or propose a feature by opening a GitHub issue. For a bug,
  include the steps to reproduce it and what you expected to happen.
- Fix or extend documentation under `docs/`.
- Write or port a plugin. A plugin is a WASM component built against the
  plugin SDK and the plugin contract (AST visitor events, scope/binding
  queries, diagnostics with `code`, `message`, `range`, `severity` and an
  optional `fix`); a sample plugin ports one real Pylint checker as a
  template. Both are planned — the requirements in `docs/` are the contract
  to build against.
- Submit a pull request for a bug fix or a new feature. For anything larger
  than a small fix, open an issue first so the approach can be agreed before
  you invest time in it.

## Set up

Install the toolchain and dependencies as described in the
[README](README.md#requirements), then:

```bash
make install
```

This installs every tool pinned in `mise.toml` — including Rust and its
compile targets; never add a `rust-toolchain.toml`, see
[`docs/rust/compile-target-source.md`](docs/rust/compile-target-source.md) —
fetches the crate's dependencies and installs `cargo-deny`.

Run `make help` for the full list of targets.

## Rust workflow

- Build and test with the `make` targets, not raw cargo: `make build`,
  `make cargo-test`, `make cargo-test FILES=...` for a narrower run, and
  `make deny`.
- Format with `make fmt`; rustfmt decides `.rs` whitespace and does not read
  `.editorconfig`, so a reformatted editor may fail `make check`.
- Clippy runs strict: `make clippy` fails on warning because `Cargo.toml`
  turns pedantic and four restriction lints on. Lint levels live in
  `Cargo.toml`, never in flags; see
  [`docs/rust/clippy-lints.md`](docs/rust/clippy-lints.md).
- Keep `Cargo.toml` to settings that differ from cargo's default; see
  [`docs/rust/cargo-toml-defaults.md`](docs/rust/cargo-toml-defaults.md).

Before opening a pull request, run the gate and the tests; both MUST pass:

```bash
make check
make test
```

`make fix` applies the safe automatic fixes for what `make check` reports.
CI runs the same targets and blocks the merge when either fails.

## Commits and branches

- Commit messages follow
  [Conventional Commits](https://www.conventionalcommits.org/).
- Branch names follow
  [Conventional Branch](https://conventionalbranch.org/):
  `<type>/<description>`, for example `feat/plugin-loader` or
  `fix/normalize-description-grammar`. The branch type is one of `feat`,
  `fix`, `hotfix`, `release`, or `chore`; documentation work uses `chore/`.

A pre-commit hook and `make commits-check` enforce both; see
[`docs/conventions/commits-check.md`](docs/conventions/commits-check.md).

## Versioning

The project follows [Semantic Versioning](https://semver.org/). A pull request
that changes behavior bumps the version in `Cargo.toml` in its own commit;
maintainers tag minor and major bumps with `make release-tag` after the pull
request merges. See
[`docs/conventions/versioning.md`](docs/conventions/versioning.md).

## Documentation

`docs/` is a bundle of atomic notes: one concept per file. A new note is
added to its directory's `index.md` and to [`docs/log.md`](docs/log.md) in
the same pull request; `make docs-lint` validates the bundle. Read
[`AGENTS.md`](AGENTS.md) for the rules that apply to both people and coding
agents working in this repository.

## License

By contributing, you agree that your contribution is licensed under the
project's license; see [LICENSE](LICENSE).
