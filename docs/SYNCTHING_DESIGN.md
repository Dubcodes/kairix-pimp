# Syncthing design

Syncthing is the only synchronization engine. PIMP works offline and treats its local REST API as an optional, read-only health signal.

## Safe shared state

Synchronizing an open SQLite database is unsafe because pages and transactions can conflict. PIMP instead writes schema-2 append-only event streams partitioned by device. Each device owns one log, so ordinary concurrent edits touch different files. Event IDs make duplicate delivery harmless; device sequences and observed sequence vectors describe causality without trusting wall clocks; deterministic replay rebuilds the projection.

Syncthing conflict copies are never ingested automatically as authoritative streams. PIMP identifies `.sync-conflict-*` files and reports them. Replace-style concurrent edits retain competing values for later resolution. Tags and project memberships use causal add/remove semantics: a removal can remove only additions that device observed, so concurrent additions converge without being lost.

Replay accepts valid events surrounding a malformed record, deduplicates repeated event IDs, reports truncated files, rejects non-monotonic owned sequences, and quarantines unsupported newer schemas. A writer refuses to append to its own truncated stream rather than compounding damage. One in-process mutex serializes native catalogue writers; each appended record is flushed before success is reported.

## Incomplete transfers and status

PIMP ignores known Syncthing/browser partial names, conflict copies and private metadata. An external candidate must keep the same size and modification evidence across multiple observations spanning at least 15 seconds before hashing. Changed known files return to that process and are marked for review if their hash differs. Stability is evidence, not proof of transfer completion; pending candidates are reported, and a future watcher or Syncthing completion signal can strengthen the decision.

Native ingest does not stage inside Master Bin. It uses a device-owned hidden sibling directory on the same filesystem, records the operation locally, independently verifies bytes, rechecks the source, and atomically creates the final name without replacement. Restart recovery removes only staging files named by validated PIMP journal records.

This is outside synchronization when DCM Master Bin itself is the Syncthing folder, which is the supported arrangement. If a broader parent directory is synchronized, its `.DCM Master Bin.pimp-staging-*` sibling must be explicitly excluded; PIMP cannot infer every external Syncthing folder definition. The application fails closed if it cannot create same-filesystem sibling staging.

Optional REST reads use a user-supplied API key stored in the OS credential store. Useful endpoints are `/rest/system/status`, `/rest/db/status`, and filtered `/rest/events`. API absence never blocks ingest or browsing. Expensive status calls are throttled.

## Rename, case, and migration handling

Renames/moves remain planned explicit events against stable IDs. Before materializing or resolving a path, PIMP rejects traversal and validates canonical containment, including symlink/junction escape where the platform exposes it. Schema readers adapt supported v1 TypeScript/Rust records, reject unsupported newer versions visibly, and never rewrite other devices' logs. Legacy `catalogue.json` is migrated once into deterministic bootstrap events; the source and a recovery backup remain unchanged. Project manifests are portable projections, not competing authoritative databases.
