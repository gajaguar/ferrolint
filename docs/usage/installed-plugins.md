---
type: guide
title: Plugins installed with pip
description: How ferrolint finds a plugin shipped in a Python package through the ferrolint.plugins entry point, and what a plugin can change in the host's findings and options.
tags: [plugins]
status: stable
---

# Plugins installed with pip

A package can ship a plugin without the user writing a `[[plugins]]` entry:
the `.wasm` goes inside the wheel and the package declares an entry point of
the `ferrolint.plugins` group in its `entry_points.txt`:

```ini
[ferrolint.plugins]
pylint_django = ferrolint_django
```

The name is the Pylint module the plugin replaces; the value is the Python
package that holds `plugin.wasm`.

## When it loads

* ferrolint reads the `*.dist-info/entry_points.txt` files of the
  `site-packages` of `VIRTUAL_ENV`. Without that variable nothing is found.
* A plugin loads only when `load-plugins` names its entry point, so a Pylint
  configuration such as `load-plugins=pylint_django` works unchanged and
  `E0013` does not fire for it.
* There is no SHA-256 pin: pip is the trust. A `[[plugins]]` entry keeps the
  pin for plugins outside pip.

## What a plugin can change

The contract 1.1.0 lets a plugin act on the host as well as report (see
[Plugin contract](../requirements/plugin-contract.md)):

* `contributions` appends values to a host option. `good-names` keeps the
  default `i,j,k,ex,Run,_` and adds the plugin's names after it.
* `augment.suppress` drops host findings of a file, by position. It gets the
  file's AST besides its source.
* `project.check-project` sees all the files of the run, each with its AST;
  a file that does not parse is left out. Its findings are cached with the
  project phase, keyed by the plugin's SHA-256 and options.

A `# pylint: disable` pragma in a file silences the findings of
`check-project` about that file.
