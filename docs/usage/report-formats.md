---
type: reference
title: Report formats
description: What each --format prints - text, json, sarif and junit - and the order every format shares.
tags: [output]
status: stable
---

# Report formats

`--format` picks one of four packagings of the same findings. Every format
lists them in one order: path, then start offset, end offset, code and
message, whatever the thread count or plugin order.

| Format           | Shape                                                                                          |
| :--------------- | :--------------------------------------------------------------------------------------------- |
| `text` (default) | `path:line:column: severity CODE (msg-id, symbol) message`, then `N files linted, M findings.` |
| `json`           | `{"diagnostics": [...], "summary": {"files": N, "findings": M}}`, keys sorted, no whitespace   |
| `sarif`          | one SARIF 2.1.0 run, every known rule as a descriptor, one result per finding                  |
| `junit`          | one `testcase` per linted file, one `failure` per finding, typed by its code                   |

* Lines and columns start at 1; a column counts characters, and SARIF
  declares that with `columnKind: unicodeCodePoints`. JSON also carries
  each position's byte offset.
* A finding of a rule that ports a Pylint check carries its msg-id and
  symbol: in parentheses in text and JUnit, as `aliases` in JSON, and in
  each SARIF result's `properties.aliases`.
* A suggested fix appears as `fix` in JSON and as `fixes` in SARIF; the
  linter never applies it.
* SARIF maps `info` to the `note` level.
