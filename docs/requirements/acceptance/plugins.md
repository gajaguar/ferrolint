---
type: rule
title: Acceptance criteria - plugins
description: Given/When/Then criteria for the plugin contract and the sample plugin, mapped to FR-03 and FR-04.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: plugins

## AC-07: A plugin receives AST visitor events (FR-03)

* Given a plugin built against the [contract](../plugin-contract.md)
  that records every visitor event it receives for a file,
* when a run lints a fixture with a function, a class and nested
  statements,
* then the recorded events cover the module body, the class and the
  function nodes in source order, with no visit after the file event
  ends.

## AC-08: Scope and binding queries resolve names (FR-03)

* Given a plugin that, at each name used, queries the host for the
  bindings visible and for the resolution target,
* when it lints a fixture with a module global, a local that shadows it
  and an import inside a function,
* then the query answers the local, the global, and the import
  origin respectively - never the shadowed binding from the outer scope.

## AC-09: Diagnostic records carry the full field set (FR-03)

* Given a plugin returning one diagnostic with severity, message, code,
  range and fix data, and another without fix data,
* when a run lints with `--format json`,
* then both diagnostics appear with exactly the fields above, the fix is
  a suggestion object the host reports, and the run's exit code is 1.

## AC-10: The sample plugin matches its real Pylint checker (FR-04)

* Given the sample plugin ported from the designated real Pylint
  checker, and the fixture files annotated with the checker's own
  findings on them,
* when the run lints the fixtures and resolves the findings through the
  [code mapping](../pylint-code-mapping.md),
* then every expected finding appears with the mapped code and message
  equivalent, no unexpected findings appear, and the assertion is part
  of `make test`.
