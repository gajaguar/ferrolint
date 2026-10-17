---
type: playbook
title: Pylint comparison on a codebase
description: How make pylint-compare lists the findings that ferrolint and Pylint do not share on a codebase, and which differences are expected on a codebase.
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

Pylint takes minutes on a large codebase, so keep `PYLINT_OUT` and delete it
when the code or the Pylint configuration changes.

## What it compares

Each finding becomes `path:line:Pxxxx`, with the Pylint code that the rule
declares, and each side is reduced to a set. The tool prints `solo-ferrolint`
and `solo-pylint` with their counts and exits with 1 if either has a finding.
It compares the line, not the column or the message.

## Differences to expect on a codebase

A project may disable `redefined-builtin` in its `.pylintrc`, and the temporary
configuration does not read that file yet, so run it with `IGNORE=NAM002`.
With `PY_VERSION=3.14` the run reports no differences: both sides agree.
