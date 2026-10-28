# Architecture

## Current status and boundaries

PIMP uses a Tauri 2 desktop shell, a React/TypeScript frontend, portable TypeScript domain logic, and narrow native adapters. The Node adapter remains the detailed automated-test reference for filesystem behavior. The compiled Rust/Tauri host exposes only settings, catalogue, ingest, metadata, project, rescan and known-media path operations; normal UI state comes from the persisted catalogue rather than fixtures.

Tauri was selected over Electron for a smaller runtime distribution and explicit capability boundaries. React supports composable accessible UI. Rust is reserved for trusted host operations and durable orchestration; FFmpeg remains an external child process invoked with explicit argument arrays.

```text
┌──────────────── React UI / WebView ────────────────┐
│ views, shared searchable controls, progress/errors │
└──────────────────────┬─────────────────────────────┘
                       │ typed commands/events (planned)
┌──────────────────────▼─────────────────────────────┐
│ application services + background job coordinator │
└──────────────┬──────────────────┬──────────────────┘
               │                  │
     ┌─────────▼────────┐  ┌──────▼─────────────────┐
     │ portable core    │  │ trusted host adapter   │
     │ rules + records  │  │ Tauri Rust / Node ref │
     └─────────┬────────┘  └──────┬─────────────────┘
               │                  │
     shared event records    filesystem, FFmpeg,
     + project manifests     optional Syncthing status
```

The frontend never receives broad arbitrary-filesystem permission. A host command accepts relative paths or user-selected handles, canonicalizes them, enforces configured roots, and returns structured progress/errors.

## Layer responsibilities

- `apps/desktop`: presentation and local interaction state. `SearchableComboBox` is the shared stable-ID selector for growing lists; Ariakit supplies ARIA/keyboard/focus/popover primitives while PIMP supplies filtering, result caps, explicit creation, and styling.
- `packages/core`: platform-neutral identities, catalogue events/projection, search, date rules, and optional AI contracts. It cannot import browser, Node, Tauri, OS, or model dependencies.
- `packages/node-adapter`: tested reference filesystem/process implementation for atomic records, safe ingest, manifests, FFprobe, and thumbnails.
- `src-tauri`: production trust boundary for purpose-built commands, event replay/migration, capability enforcement, safe ingest/discovery, OS integration, and minimal durable operation records.
- future adapters/workers: local SQLite/FTS projection, Syncthing read-only status, optional Full AI workers, and Premiere UXP extension.

## Ingest and background jobs

The implemented single-file reference transaction:

1. Snapshot source metadata and select capture date with provenance.
2. Resolve a destination under `media/YYYY-MM-DD` and enforce its canonical root.
3. Recognize an identical hash as a duplicate; reject different content at an existing path.
4. Record the operation in a device-local journal and stream to an application-owned staging sibling outside the synchronized Master Bin.
5. Recheck source evidence and hash, independently hash staged bytes, then atomically create the destination name without replacement.
6. Append and flush `media.registered` only after successful promotion. A persistence failure leaves a verified orphan that conservative discovery can recover; it never rolls back by deleting the media.
7. Probe and preview as separately retryable work; their failure does not invalidate the verified ingest.

Long work executes outside the render thread with stable job IDs, progress events and cancellation. A lifecycle journal remains through verified promotion and is removed only after catalogue persistence; restart recovery verifies the promoted path, size, checksum and catalogue identity before registration, and never removes a promoted destination. Ambiguous recovery state remains intact for review. Byte-offset resume and retry scheduling remain future work. Event handlers are idempotent because reconnect/replay can redeliver state.

The target job architecture persists versioned intent, progress and retry state for ingest, enrichment, consolidation and cleanup, then resumes eligible work at startup. Ingest intent will include initial tags, projects, description and favourite state without making incomplete media visible in the committed catalogue.

FFprobe and thumbnail enrichment run sequentially only for records missing required metadata or cached previews and cannot invalidate a verified copy. External discovery persists observations, waits for repeated evidence across at least 15 seconds, rechecks previously indexed files when size/timestamp evidence changes, and hashes only candidates requiring verification. A native notification watcher remains pending.

## Metadata, search, and Syncthing

The synchronized Master Bin contains portable relative paths, stable IDs and device-partitioned append-only schema-2 event logs. Devices never write another device's stream. A deterministic topological replay respects device sequence and observed-vector edges, then applies stable ordering only to concurrent events. Integrity observations are retained by device. The legacy snapshot is retained for recovery but is no longer operational state.

```text
device A events ─┐
device B events ─┼─ Syncthing ─► validated replay ─► local projection
device C events ─┘                                  ├─ in-memory state (now)
                                                    └─ SQLite/FTS (planned)
```

SQLite/FTS is a disposable device-local search projection, never a synchronized live database. Rebuilding it reads validated authoritative events plus media availability. The open Library polls a cheap event-file signature and rebuilds only after the shared streams change; media discovery remains a separate operation. Syncthing temporary and conflict files are detected and reported, not silently treated as authoritative. Its optional local REST API supplies read-only health hints; PIMP works without it. Detailed rules are in [SYNCTHING_DESIGN.md](SYNCTHING_DESIGN.md).

## Identity and project architecture

Stable media IDs are independent of content hashes, names and paths; SHA-256 remains integrity and duplicate evidence. Folder and project IDs are likewise path-independent. Portable records use `/`-separated relative paths. Device settings map Master Bin and Working Projects roots locally.

Media contains `projectIds[]`, so assignments are many-to-many. Each working directory owns an atomically updated `.pimp-project.json` manifest containing portable media identities, paths, checksums, and relevant metadata without recreating or deleting existing assets. See [DATA_MODEL.md](DATA_MODEL.md).

## Optional AI boundary

Full-edition analysis runs behind an optional process-isolated `AiProcessor` contract. A worker streams timestamped results with model ID/version, confidence where meaningful, cache identity, and provenance. Jobs require pause/resume/cancel, resource limits, retry policy, and failure isolation. Light may display compatible existing results but imports and ships no inference runtime or model. Model output never silently becomes manual fact.

## Premiere and archive boundaries

A future UXP extension uses documented active-project, import, folder/bin, and media-path APIs. PIMP reconciles Premiere's observed contents with its manifest and never edits `.prproj` internals. The extension and desktop app communicate through a versioned, authenticated local boundary still to be designed and tested.

Until that extension exists, the default editing workflow references footage directly from Master Bin. Working Project folders and their existing Premiere saves/assets are not restructured, and users continue to use Premiere's normal desktop/laptop drive-path relinking.

The established product layout calls that source root **DCM Master Bin**. Dated folders become Premiere bins and an optional virtual Audio grouping may expose paired audio without moving it. A future explicit collection workflow writes a verified portable archive under DCM Content; only its successful state transition makes the collected `.prproj` authoritative for the stable project ID.

Finalization builds a dependency inventory, copies or derives an archive, verifies integrity, and validates that the Premiere project resolves independently of the Master Bin. Complete-source mode precedes consolidation/transcode. Derived media gets a new checksum plus derivation/time-range provenance rather than pretending to match the source hash. Cleanup eligibility is a separate stage that considers every active project, Syncthing state, explicit authorization, and recoverable quarantine.

Completed projects retain lightweight searchable catalogue records even when archive media is offline. Future cleanup automation remains disabled by default, honors per-file protection, treats Syncthing peers as replicas rather than backups, and cannot bypass verification or recoverable quarantine.

## Errors and recovery

- Use structured error codes plus actionable user messages; preserve causal detail in local logs without secrets.
- Fail closed on path, checksum, schema, archive independence, and destructive-operation uncertainty.
- Atomic writes and per-device append-only events protect authoritative data across interruption.
- Keep failed jobs inspectable and retryable; cleanup only PIMP-owned temporary files after boundary verification.
- Quarantine malformed/newer metadata and Syncthing conflicts instead of silently dropping or rewriting them.

## Security boundaries

- Tauri capabilities expose only required commands to approved windows.
- Canonicalize and validate filesystem roots, symlinks/junctions, reserved names, Unicode, and case collisions.
- Invoke processes without shell interpolation; constrain executable discovery and arguments.
- Store a future Syncthing API key in the OS credential store and never in synchronized metadata/logs.
- Treat media metadata, AI output, event files, Premiere data, and external tool output as untrusted input.
- Destructive actions require distinct review and explicit user authorization; none exist in the current milestone.

## Cross-platform considerations

Windows/WebView2 is first. Core records preserve canonical spelling while detecting case-fold hazards, never store drive letters as portable identity, and isolate file watching, removable-device discovery, process invocation, credential storage, and path rules behind adapters. Linux WebKit/FFmpeg packaging and macOS signing/notarization/sandbox behavior need later platform-specific implementations and tests; portability in design is not a verified release claim.
