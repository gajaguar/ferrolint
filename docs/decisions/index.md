# Decisions

Each note records one decision: the context that forced it, the options
considered, what was chosen, and what the choice puts on future us.

* [Extensible linter over a Ruff fork](extensible-linter-over-ruff-fork.md) -
  ferrolint runs only WASM-plugin rules next to Ruff instead of forking Ruff
  or rewriting Pylint in Rust; Ruff's parser crates are pinned by tag.
