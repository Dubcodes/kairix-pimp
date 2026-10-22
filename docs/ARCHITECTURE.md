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
- `src-tauri`: planned production trust boundary for purpose-built commands, capability enforcement, OS integration, and durable job control.
- future adapters/workers: local SQLite/FTS projection, Syncthing read-only status, optional Full AI workers, and Premiere UXP extension.

## Ingest and background jobs

The implemented single-file reference transaction:

1. Snapshot source metadata and select capture date with provenance.
2. Resolve a destination under `media/YYYY-MM-DD` and enforce its canonical root.
3. Recognize an identical hash as a duplicate; reject different content at an existing path.
4. Hash the source, stream to a unique sibling `.pimp-part-*` file, independently hash it, compare, then atomically rename.
5. Append `media.registered` only after successful promotion.
6. Probe and preview as separately retryable work; their failure does not invalidate the verified ingest.

Recursive ingest needs a durable journal. Long work must execute outside the render thread with stable job IDs, item/progress events, cancellation, bounded concurrency, retries, and restart recovery. Event handlers must be idempotent because reconnect/replay can redeliver state.

The current host implements stable job IDs, byte/file progress, cancellation and partial-file cleanup, but the durable journal and restart recovery remain pending. FFprobe and thumbnail enrichment run after successful ingest and cannot invalidate a verified copy. External discovery uses a conservative stable-file scan every 30 seconds and an explicit Rescan command; a native notification watcher is still pending.

## Metadata, search, and Syncthing

The synchronized Master Bin contains portable relative paths, stable IDs, a small catalogue descriptor, and device-partitioned append-only event logs. Devices never write another device's stream. Deterministic replay deduplicates event IDs and retains ambiguous concurrent updates for resolution.

```text
device A events ─┐
device B events ─┼─ Syncthing ─► validated replay ─► local projection
device C events ─┘                                  ├─ in-memory state (now)
                                                    └─ SQLite/FTS (planned)
```

SQLite/FTS is a disposable device-local search projection, never a synchronized live database. Rebuilding it reads validated authoritative events plus media availability. Syncthing temporary and conflict files are detected and reported, not silently treated as authoritative. Its optional local REST API supplies read-only health hints; PIMP works without it. Detailed rules are in [SYNCTHING_DESIGN.md](SYNCTHING_DESIGN.md).

## Identity and project architecture

Content hashes currently provide stable media identity; folder and project IDs are independent of names and paths. Portable records use `/`-separated relative paths. Device settings map Master Bin and Working Projects roots locally.

Media contains `projectIds[]`, so assignments are many-to-many. Each working directory owns an atomically updated `.pimp-project.json` manifest containing portable media identities, paths, checksums, and relevant metadata without recreating or deleting existing assets. See [DATA_MODEL.md](DATA_MODEL.md).

## Optional AI boundary

Full-edition analysis runs behind an optional process-isolated `AiProcessor` contract. A worker streams timestamped results with model ID/version, confidence where meaningful, cache identity, and provenance. Jobs require pause/resume/cancel, resource limits, retry policy, and failure isolation. Light may display compatible existing results but imports and ships no inference runtime or model. Model output never silently becomes manual fact.

## Premiere and archive boundaries

A future UXP extension uses documented active-project, import, folder/bin, and media-path APIs. PIMP reconciles Premiere's observed contents with its manifest and never edits `.prproj` internals. The extension and desktop app communicate through a versioned, authenticated local boundary still to be designed and tested.

Until that extension exists, the default editing workflow references footage directly from Master Bin. Working Project folders and their existing Premiere saves/assets are not restructured, and users continue to use Premiere's normal desktop/laptop drive-path relinking.

Finalization builds a dependency inventory, copies or derives an archive, verifies integrity, and validates that the Premiere project resolves independently of the Master Bin. Complete-source mode precedes consolidation/transcode. Derived media gets a new checksum plus derivation/time-range provenance rather than pretending to match the source hash. Cleanup eligibility is a separate stage that considers every active project, Syncthing state, explicit authorization, and recoverable quarantine.

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
