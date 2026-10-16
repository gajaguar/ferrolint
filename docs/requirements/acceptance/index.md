# Acceptance criteria

Atomic Given/When/Then criteria for the [requirements](../index.md),
with stable IDs. Each note covers one acceptance area; each criterion
maps to the functional or non-functional requirement it verifies.
[`ac-index.md`](../ac-index.md) is the flat ID list a test task works
from.

* [Configuration](configuration.md) - configuration sources, plugin SHA
  verification, select and ignore, discovery filters.
* [Plugins](plugins.md) - contract events, scope queries, diagnostics,
  sample plugin parity.
* [Suppressions](suppressions.md) - the three comment forms.
* [Output](output.md) - the four formats and code aliases.
* [Exit codes](exit-codes.md) - the 0, 1 and 2 paths.
* [Diff mode](diff-mode.md) - changed-file selection.
* [Cache](cache.md) - replay and invalidation.
* [Performance](performance.md) - budgets under load.
* [Sandbox](sandbox.md) - no escape, no hang.
* [Determinism](determinism.md) - byte-identical reruns.
* [Platforms](platforms.md) - targets, pinning, gate.
