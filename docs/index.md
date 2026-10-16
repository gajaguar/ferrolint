---
okf_version: "0.2"
---

# Documentation

This is an [Open Knowledge Format](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md)
(OKF) bundle: one Markdown concept per file, each with YAML frontmatter, laid
out under this directory.

## Reference

* [Conventions](conventions/index.md) - commit and branch naming, and how
  they're enforced.
* [Toolchain](toolchain/index.md) - which layer (mise or an ecosystem
  package manager) installs which tool, and why.
* [Usage](usage/index.md) - the configuration file and the rules the
  linter reports itself.
* [Requirements](requirements/index.md) - the product's functional and
  non-functional requirements, with acceptance criteria per area.
* [Backlog](backlog.md) - the implementation epics and the criteria each closes.
* [Decisions](decisions/index.md) - the architectural decisions and what
  they put on future us.

See [`log.md`](log.md) for the bundle's change history.

## Rust

* [Rust](rust/index.md) - the compile-target source, the `Cargo.toml`
  settings left out on purpose, the Clippy lints and `cargo-deny`.
