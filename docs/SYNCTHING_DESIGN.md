# Syncthing design

Syncthing is the only synchronization engine. PIMP works offline and treats its local REST API as an optional, read-only health signal.

## Safe shared state

Synchronizing an open SQLite database is unsafe because pages and transactions can conflict. PIMP instead writes append-only event streams partitioned by device. Each device owns one log, so ordinary concurrent edits touch different files. Event IDs make duplicate delivery harmless; deterministic replay rebuilds the projection.

Syncthing conflict copies are never ingested automatically as authoritative streams. PIMP identifies `.sync-conflict-*` files, reports them, and offers a later import/reconciliation flow. Replace-style concurrent edits are surfaced. Set-like metadata can later use add/remove operations with causal context.

## Incomplete transfers and status

PIMP ignores Syncthing temporary files and PIMP `.pimp-part-*` files. A candidate must be stable across observations or accompanied by an ingest completion event before indexing. Missing files remain catalogue records with an offline/missing state.

Optional REST reads use a user-supplied API key stored in the OS credential store. Useful endpoints are `/rest/system/status`, `/rest/db/status`, and filtered `/rest/events`. API absence never blocks ingest or browsing. Expensive status calls are throttled.

## Rename, case, and migration handling

Renames/moves are explicit events against stable IDs. On case-insensitive systems, case-only renames use an intermediate name. Before materializing a remote path, PIMP checks path normalization, reserved names, Unicode equivalence, and case-fold collisions. Schema readers accept supported older versions, reject unsupported newer versions visibly, and never rewrite other devices' logs.
