---
type: playbook
title: Pylint comparison on a codebase
description: How make pylint-compare lists the findings that ferrolint and Pylint do not share on a codebase, and which differences are expected.
tags:
  - toolchain
  - makefile
status: stable
---

# Pylint comparison on a codebase

`make pylint-compare` lints a directory with every compiled plugin and with
Pylint, then prints the findings only one of them reports. It is how the
parity with Pylint on a codebase is measured; the tool is `tools/pylint-compare.sh`.

## Run it

Build the plugins and the binary first (`make plugins build`), then:

```sh
make pylint-compare CODE=/path/to/code PYLINT=/path/to/pylint \
  PYLINT_OUT=/tmp/pylint.out IGNORE=NAM002 PY_VERSION=3.14
```

* `CODE`: the directory to lint. Both linters run inside it, so Pylint reads
  its own `.pylintrc`.
* `PYLINT`: the Pylint command, which may be several words, so it can set the
  environment Pylint needs: `PYLINT="env DJANGO_SETTINGS_MODULE=app.settings
  pylint"`. Defaults to `pylint`. The tool fails if Pylint ends without its
  rating line, since a crash prints no findings and would look like parity.
* `PYLINT_OUT`: a file with Pylint's output. If it exists it is reused; if
  not, Pylint writes it.
* `IGNORE`: ferrolint rule codes, separated by commas, that the temporary
  configuration ignores.
* `PY_VERSION`: the `py-version` of the temporary configuration, like
  `3.14`.
* `STRIP`: set to `1` to lint a copy of the tracked files in `CODE` (and its
  `.pylintrc` and `pyproject.toml`), from which every `# pylint:` pragma
  comment is
  removed from the `*.py` files (line numbers stay the same; `# noqa` stays). The
  comments are found with `python3` (`tools/pylint-strip.py`), so the same text
  inside a string stays. `make pylint-compare-test` checks this on a fixture. Pragmas
  hide nearly every finding, so this is how a rule is compared where
  it fires. `PYLINT_OUT` then holds the copy's baseline: use a file other than
  the normal one. With `BIN` too, ferrolint runs with no temporary
  configuration: it reads the copy's `.pylintrc`, options included, like a project
  does, and `PY_VERSION` is appended to it; `IGNORE` has no effect.
* `SELECT`: comma-separated rule codes for the temporary configuration's
  `select`, like `I0011,I0013`. Pylint's informational pragma messages are off
  by default in both linters: run Pylint with `--enable=I` in `PYLINT` and
  name them here. It has no effect with `STRIP` and `BIN`.
* `ONLY`: an extended regular expression on the Pylint msg-id, like
  `^(W0134|W0137)$` or `^W01`; only matching findings are compared, on both
  sides.
* `MSG`: set to `1` to compare the message as well as the line. Pylint then
  runs with a template that adds its message, and each finding becomes
  `path:line:Pxxxx:message`, with whitespace collapsed on both sides. A
  `PYLINT_OUT` reused with `MSG=1` must come from a `MSG=1` run, since
  a file without messages yields no Pylint findings.

Pylint takes minutes on a large codebase, so keep `PYLINT_OUT` and delete it
when the code or the Pylint configuration changes.

## What it compares

Each finding becomes `path:line:Pxxxx`, with the Pylint code that the rule
declares, and each side is reduced to a set. The tool prints `solo-ferrolint`
and `solo-pylint` with their counts and exits with 1 if either has a finding.
It compares the line, not the column; `MSG=1` adds the message, so a finding
whose message differs in wording shows up as a solo finding on each side.

A finding in a configuration file is compared too, as `.pylintrc:1:E0015`:
both tools put it on line 1 of the file. Pylint reports `E0013` under the
module name `Command line or configuration file` instead of a path; the script
rewrites that name to `.pylintrc`. The temporary configuration of a run
without `BIN` and `STRIP` does not read `.pylintrc`, so only a bundled `BIN`
with `STRIP` compares these.

## Differences to expect

A project that disables `redefined-builtin` in its `.pylintrc` needs
`IGNORE=NAM002` when the temporary configuration is used, since that
configuration does not read the file. A bundled `BIN` with `STRIP` reads it,
so it needs no `IGNORE`.
