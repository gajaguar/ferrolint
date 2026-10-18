---
type: reference
title: AC index
description: Static list of every acceptance criterion id, its note and the requirement it verifies.
tags: [requirements, testing]
status: stable
---

# AC index

The flat list of acceptance criteria; a test task works from this list.
Every criterion is Given/When/Then in its note; the requirement hook is
the parenthesised FR or NFR.

| ID    | Note                                          | Verifies |
| :---- | :-------------------------------------------- | :------- |
| AC-01 | [Configuration](acceptance/configuration.md)  | FR-02    |
| AC-02 | [Configuration](acceptance/configuration.md)  | FR-02    |
| AC-03 | [Configuration](acceptance/configuration.md)  | FR-02    |
| AC-04 | [Configuration](acceptance/configuration.md)  | FR-02    |
| AC-05 | [Configuration](acceptance/configuration.md)  | FR-01    |
| AC-06 | [Configuration](acceptance/configuration.md)  | FR-01    |
| AC-07 | [Plugins](acceptance/plugins.md)              | FR-03    |
| AC-08 | [Plugins](acceptance/plugins.md)              | FR-03    |
| AC-09 | [Plugins](acceptance/plugins.md)              | FR-03    |
| AC-10 | [Plugins](acceptance/plugins.md)              | FR-04    |
| AC-11 | [Suppressions](acceptance/suppressions.md)    | FR-05    |
| AC-12 | [Suppressions](acceptance/suppressions.md)    | FR-05    |
| AC-13 | [Suppressions](acceptance/suppressions.md)    | FR-05    |
| AC-14 | [Output](acceptance/output.md)                | FR-06    |
| AC-15 | [Output](acceptance/output.md)                | FR-06    |
| AC-16 | [Output](acceptance/output.md)                | FR-06    |
| AC-17 | [Output](acceptance/output.md)                | FR-06    |
| AC-18 | [Output](acceptance/output.md)                | FR-10    |
| AC-19 | [Exit codes](acceptance/exit-codes.md)        | FR-07    |
| AC-20 | [Exit codes](acceptance/exit-codes.md)        | FR-07    |
| AC-21 | [Exit codes](acceptance/exit-codes.md)        | FR-07    |
| AC-22 | [Diff mode](acceptance/diff-mode.md)          | FR-08    |
| AC-23 | [Diff mode](acceptance/diff-mode.md)          | FR-08    |
| AC-24 | [Cache](acceptance/cache.md)                  | FR-09    |
| AC-25 | [Cache](acceptance/cache.md)                  | FR-09    |
| AC-26 | [Performance](acceptance/performance.md)      | NFR-01   |
| AC-27 | [Performance](acceptance/performance.md)      | NFR-05   |
| AC-28 | [Performance](acceptance/performance.md)      | NFR-02   |
| AC-29 | [Sandbox](acceptance/sandbox.md)              | NFR-03   |
| AC-30 | [Sandbox](acceptance/sandbox.md)              | NFR-03   |
| AC-31 | [Determinism](acceptance/determinism.md)      | NFR-04   |
| AC-32 | [Platforms](acceptance/platforms.md)          | NFR-06   |
| AC-33 | [Platforms](acceptance/platforms.md)          | NFR-07   |
| AC-34 | [Platforms](acceptance/platforms.md)          | NFR-08   |

FR-11 (the type inference query) carries no criterion in this index: see
[Type inference query](type-inference-query.md) for what it covers and what
stays silent when inference gives up.
