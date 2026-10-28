# Syncthing design

Syncthing is the only synchronization engine. PIMP works offline and treats its local REST API as an optional, read-only health signal.

## Safe shared state

Synchronizing an open SQLite database is unsafe because pages and transactions can conflict. PIMP instead writes schema-2 append-only event streams partitioned by device. Each device owns one log, so ordinary concurrent edits touch different files. Event IDs make duplicate delivery harmless; device sequences and observed sequence vectors describe causality without trusting wall clocks; deterministic replay rebuilds the projection.

Syncthing conflict copies are never ingested automatically as authoritative streams. PIMP identifies `.sync-conflict-*` files and reports them. Replace-style concurrent edits retain competing values for later resolution. Tags and project memberships use causal add/remove semantics: a removal can remove only additions that device observed, so concurrent additions converge without being lost.

Replay accepts valid events surrounding a malformed record, deduplicates repeated event IDs, reports and excludes an unterminated final record, rejects non-monotonic owned sequences, and quarantines unsupported newer schemas. Topological ordering enforces all available causal edges. A writer refuses to append to its own truncated stream rather than compounding damage. One in-process mutex serializes commands and the desktop single-instance guard prevents a second local writer; each appended record is flushed before success is reported.

Concurrent registrations from offline devices are reconciled during projection, not by rewriting synchronized logs. Equal normalized relative path plus equal SHA-256 identifies duplicate registrations of one synchronized recording; deterministic causal ordering selects the canonical historic ID and aliases all other IDs for subsequent metadata and project events. Equal content at different paths remains separate. Equal paths with different hashes remain separate and surface an identity conflict. Rust and TypeScript reducers apply the same rule.

While the Library is open, the host exposes a signature derived from event-stream filenames, sizes and modification evidence. The UI polls that signature and reloads the catalogue only when it changes, so remote tag, project and conflict events appear without invoking media discovery or requiring the Syncthing REST API. A separate device-local availability poll checks only paths already in the projection. When Syncthing materializes one of those files, the Library refreshes without appending a shared event or asserting global integrity.

## Incomplete transfers and status

PIMP ignores known Syncthing/browser partial names, conflict copies and private metadata. An external candidate must keep the same size and modification evidence across multiple observations spanning at least 15 seconds before hashing. Changed known files return to that process and are marked for review if their hash differs. Stability is evidence, not proof of transfer completion; pending candidates are reported, and a future watcher or Syncthing completion signal can strengthen the decision.

Native ingest does not stage inside Master Bin. It uses a device-owned hidden sibling directory on the same filesystem, records the operation locally, independently verifies bytes, rechecks the source, and atomically creates the final name without replacement. The device-local persistent queue reserves planned destinations across active jobs and is never synchronized. Restart recovery first establishes that no worker is active, marks unfinished jobs interrupted, and then removes only staging files named by validated PIMP journal records. It does not automatically restart copying from removable media.

This is outside synchronization when DCM Master Bin itself is the Syncthing folder, which is the supported arrangement. If a broader parent directory is synchronized, its `.DCM Master Bin.pimp-staging-*` sibling must be explicitly excluded; PIMP cannot infer every external Syncthing folder definition. The application fails closed if it cannot create same-filesystem sibling staging.

For a broader synchronized parent, add the root-relative pattern
`/.DCM Master Bin.pimp-staging-*` to that folder's `.stignore`. The preferred
configuration needs no exclusion because DCM Master Bin itself is the Syncthing
folder and the staging sibling is outside it. See [BETA_TESTING.md](BETA_TESTING.md)
for the disposable two-device acceptance procedure.

Optional REST reads use a user-supplied API key stored in the OS credential store. Useful endpoints are `/rest/system/status`, `/rest/db/status`, and filtered `/rest/events`. API absence never blocks ingest or browsing. Expensive status calls are throttled.

## Rename, case, and migration handling

Renames/moves remain planned explicit events against stable IDs. Before materializing or resolving a path, PIMP rejects traversal and validates canonical containment, including symlink/junction escape where the platform exposes it. Schema readers adapt supported v1 TypeScript/Rust records, reject unsupported newer versions visibly, and never rewrite other devices' logs. Legacy `catalogue.json` is migrated into one deterministic, source-hash-verified bootstrap stream; the source and a recovery backup remain unchanged. Absence of a legacy snapshot creates no shared completion marker, so a snapshot arriving later can still migrate. Project manifests are portable projections, not competing authoritative databases. Failed projection writes are retained in a device-local reconciliation queue and retried after restart; a temporary unavailable project drive cannot roll back or duplicate catalogue events.
