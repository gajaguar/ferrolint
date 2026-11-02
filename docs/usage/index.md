# Usage

How to run ferrolint and read what it reports: the parts a reader cannot get
from `ferrolint --help` alone.

* [Configuration file](configuration-file.md) - the keys of `ferrolint.toml`
  and `[tool.ferrolint]`, and how paths and globs resolve.
* [Report formats](report-formats.md) - text, json, sarif and junit, and
  the order they share.
* [Suppression comments](suppression-comments.md) - the three comment forms
  and the line they apply to.
* [Host rules](host-rules.md) - the `FL` codes the linter reports itself,
  without any plugin.
* [Native rules](native-rules.md) - the Pylint rules the host runs itself, by
  batch, and where they differ from Pylint.
* [Rules that wait for type inference](inference-dependent-rules.md) - the
  differences from Pylint that only type inference closes, and ty.
* [Rules answered by ty](ty-rules.md) - the operation rules of a build with
  the `ty` feature, and where they differ from Pylint.
* [Writing a plugin](writing-a-plugin.md) - the SDK's `Checker`, building to
  `wasm32-wasip2`, loading by SHA-256, and parity fixtures against Pylint.
* [A plugin outside this repository](out-of-tree-plugin.md) - depending on the
  SDK by revision, building and loading from another repository.
* [Plugins installed with pip](installed-plugins.md) - the
  `ferrolint.plugins` entry point, `load-plugins` and what a plugin changes in
  the host.
* [Result cache](result-cache.md) - what `.ferrolint-cache/` stores and
  what invalidates it.
* [Diff mode](diff-mode.md) - the files `--diff-base` lints.
* [Bundled plugins](plugins.md) - the ported Pylint checkers, their
  codes and aliases.
