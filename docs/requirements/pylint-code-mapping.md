---
type: rule
title: Pylint code mapping
description: How a Pylint message id or symbol alias translates to a plugin code, and where each is accepted.
tags: [requirements, config, output]
status: stable
---

# Pylint code mapping

`FR-10`: users speak Pylint at ferrolint because the plugins replace
Pylint's rules; a ported rule keeps its msg-id and symbol.

* A mapping table links each ported checker's plugin code to its Pylint
  msg-id (for example `C0304`) and symbol (for example `unreachable`);
  the sample plugin in SDK docs shows the table in use.
* `select` and `ignore` accept plugin codes, msg-ids and symbols
  interchangeably: each is resolved through the table before the run.
* In reports, a diagnostic carries its plugin code and its resolved
  aliases, so the output reads in Pylint terms without grounding users in
  plugin codes alone.
* Per-file-ignores use the same resolution as select and ignore; a
  suppression comment payload (see
  [Inline suppressions](inline-suppressions.md)) can therefore cite
  either spelling.
* A msg-id merged from two ports (Pylint allows it) has one plugin code
  in the table at a time: the table, not the resolution order, is the
  source of truth for which checker owns the id.
* `select`-ing an id no plugin covers is a configuration error (exit 2);
  typos fail loudly instead of silently deselecting everything.
