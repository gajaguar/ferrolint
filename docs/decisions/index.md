# Decisions

Each note records one decision: the context that forced it, the options
considered, what was chosen, and what the choice puts on future us.

* [Extensible linter over a Ruff fork](extensible-linter-over-ruff-fork.md) -
  ferrolint runs only WASM-plugin rules next to Ruff instead of forking Ruff
  or rewriting Pylint in Rust; Ruff's parser crates are pinned by tag.
* [Plugin sandbox environment](plugin-sandbox-environment.md) - WASI linked
  but emptied, frozen clocks and fixed randomness, and a fresh instance
  under its own caps for every file.
* [Packed AST](packed-ast.md) - why contract 0.2.0 sends the AST as `u32`
  words and one string blob.
* [Type inference query](type-inference-query.md) - the `inference`
  interface of contract 0.2.1 and what the host infers.
