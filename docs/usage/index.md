# Usage

How to run ferrolint and read what it reports: the parts a reader cannot get
from `ferrolint --help` alone.

* [Configuration file](configuration-file.md) - the keys of `ferrolint.toml`
  and `[tool.ferrolint]`, and how paths and globs resolve.
* [Suppression comments](suppression-comments.md) - the three comment forms
  and the line they apply to.
* [Host rules](host-rules.md) - the `FL` codes the linter reports itself,
  without any plugin.
* [Writing a plugin](writing-a-plugin.md) - the SDK's `Checker`, building to
  `wasm32-wasip2`, loading by SHA-256, and parity fixtures against Pylint.
