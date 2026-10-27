# AGENTS.md

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
"SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be
interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

## Agent instructions

- `AGENTS.md` is the only agent instructions file; put project rules here. The
  repository MUST NOT contain a `CLAUDE.md` or any other tool-specific copy,
  because a second copy drifts from this one; `make claude-md-check` fails on
  one.

## Command surface

- Run the `Makefile` targets (`make check`, `make fix`, `make test`, ...)
  instead of the underlying tools, so the agent and CI use the same options.
  `make help` lists them.
- Give every new target a `##` help line; `make help` prints it and
  `make help-check` fails on a target without one.

## Gate

- `make check` and `make test` MUST pass before any commit.
- Run `make fix` first for findings it can repair, then edit by hand.

## Commits and branches

- Write commit messages as
  [Conventional Commits](https://www.conventionalcommits.org/) and branch
  names as [Conventional Branch](https://conventionalbranch.org/)
  (`<type>/<description>`, e.g. `feat/add-login`). A pre-commit hook and
  `make commits-check` enforce both; see
  [`docs/conventions/commits-check.md`](docs/conventions/commits-check.md).
- Name a documentation or dependency branch `chore/...`: a branch type is not
  a commit type, and `docs/` is not one.

## Pull requests

Once a pull request is open, the agent MUST:

1. Wait for CI; while it fails, fix the cause, push to the same branch and
   wait again until it passes.
2. Squash-merge a pull request with exactly one commit and use a regular merge
   commit otherwise (`gh pr view --json commits` gives the count).
3. Delete the branch on the remote and locally.
4. Switch back to the base branch, pull it and run `git fetch --prune`.

## Versioning and releases

- Decide every version bump, tag and release as
  [`docs/conventions/versioning.md`](docs/conventions/versioning.md) defines;
  it is the only place those rules live.

## Publishing to PyPI

- Publish nothing before 1.0.0: the `publish` job skips a `v0.*` tag. From
  1.0.0, publish only when asked: create the GitHub Release from the `vX.Y.Z`
  tag, and approve the `pypi` environment's deployment, only on request.
- Follow [`docs/toolchain/release-builds.md`](docs/toolchain/release-builds.md)
  for the wheels, the sdist and the one-time PyPI setup.

## Documentation

- Write documentation as an OKF bundle of atomic notes under `docs/`: one
  Markdown concept per file, with YAML frontmatter (`type`, `title`,
  `description`).
- Add a new note to its directory's `index.md` and, by file name, to
  [`docs/log.md`](docs/log.md); `make docs-lint` fails on a missing field or a
  broken link.
- Write a note only when it explains something a reader cannot already get
  from `make help`, a linter's own message, or the configuration it comes
  from.

## Dependencies

- Add a new tool to the ecosystem manager that owns it; use `mise.toml` only
  for a tool that bootstraps an ecosystem or has no manager in this
  repository. See
  [`docs/toolchain/layering-rule.md`](docs/toolchain/layering-rule.md).

## Sub-agents with pi

- Launch a non-interactive pi sub-agent with standard input closed:
  `pi -p ... "<prompt>" < /dev/null`. In print mode pi waits on an open
  standard input (a pipe, a background shell), so without the redirect it
  hangs with no output, no CPU and no network connection.
- Add `--mode json` and a hard timeout
  (`perl -e 'alarm shift; exec @ARGV' <seconds> pi ...`; macOS has no
  `timeout`). The JSON log grows one event per line; treat a log that stops
  growing for 3 minutes as stalled, then kill and relaunch the run.

## Rust

- Pin the Rust version, its components and its compile targets in `mise.toml`
  only; never add `rust-toolchain.toml`, which mise does not read. See
  [`docs/rust/compile-target-source.md`](docs/rust/compile-target-source.md).
- Keep `Cargo.toml` to settings that differ from cargo's default; see
  [`docs/rust/cargo-toml-defaults.md`](docs/rust/cargo-toml-defaults.md).
- Let `make fmt` decide `.rs` whitespace: rustfmt does not read
  `.editorconfig`, so an editor that re-indents Rust to the project's two
  spaces fails `make check`.
- Set lint levels in `[lints]` in `Cargo.toml`, never as flags in `mk/rust.mk`;
  see [`docs/rust/clippy-lints.md`](docs/rust/clippy-lints.md).
- Pin `cargo-deny` in `mk/rust.mk` and install it with `cargo install`, not
  mise; `make deny` runs in `make check`. See
  [`docs/rust/cargo-deny.md`](docs/rust/cargo-deny.md).
- Tag a minor or major bump with `make release-tag` after its pull request
  merges: it tags the base branch as `v<package.version>`, read from
  `Cargo.toml`.
