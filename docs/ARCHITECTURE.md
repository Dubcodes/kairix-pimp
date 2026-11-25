# Architecture

## Current status and boundaries

PIMP uses a Tauri 2 desktop shell, a React/TypeScript frontend, portable TypeScript domain logic, and narrow native adapters. The Node adapter remains the detailed automated-test reference for filesystem behavior. The compiled Rust/Tauri host exposes only settings, catalogue, ingest, metadata, project, rescan and known-media path operations; normal UI state comes from the persisted catalogue rather than fixtures. Windows Explorer actions accept a media ID, reload the catalogue, canonically resolve the stored relative path beneath Master Bin and reject unavailable or escaping paths before calling `SHOpenFolderAndSelectItems`; the webview never supplies an arbitrary path.

Tauri was selected over Electron for a smaller runtime distribution and explicit capability boundaries. React supports composable accessible UI. Rust is reserved for trusted host operations and durable orchestration; FFmpeg remains an external child process invoked with explicit argument arrays.

```text
┌──────────────── React UI / WebView ────────────────┐
│ views, shared searchable controls, progress/errors │
└──────────────────────┬─────────────────────────────┘
                       │ typed commands/events
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
- `src-tauri`: production trust boundary for purpose-built commands, event replay/migration, capability enforcement, safe ingest/discovery, OS integration, durable ingest jobs and operation journals.
- future adapters/workers: local SQLite/FTS projection, Syncthing read-only status, optional Full AI workers, and Premiere UXP extension.

## Ingest and background jobs

The implemented single-file reference transaction:

1. Inventory every discovered source file, report exclusions/read failures, classify analysis support, and select capture date with provenance.
2. Resolve a destination under `media/YYYY-MM-DD` and enforce its canonical root.
3. Recognize an identical hash as a duplicate; reject different content at an existing path.
4. Record the operation in a device-local journal and stream to an application-owned staging sibling outside the synchronized Master Bin.
5. Recheck source evidence and hash, independently hash staged bytes, then atomically create the destination name without replacement.
6. Append and flush `media.registered` only after successful promotion. A persistence failure leaves a verified orphan that conservative discovery can recover; it never rolls back by deleting the media.
7. Probe and preview as separately retryable work; their failure does not invalidate the verified ingest.

Long work executes outside the render thread through a device-local versioned job store. Jobs retain source inventory totals, source-relative paths, deliberate exclusions, inventory issues, destination reservations, per-file and aggregate progress, failures, and provisional tags, projects, notes and favourite state. Camera-package paths and duplicate basenames preserve hierarchy; when separately selected roots would produce the same relative path, a deterministic unique source-root directory is retained. The inventory preview and queued job use the same relative-path planner. Ordinary unique selections still use the compact date/filename layout. In original-file mode, independent files use their own modification times, while all members of a detected package use the earliest available member modification time and therefore remain in one date folder. One bounded worker processes jobs sequentially. Copying and hashing occur without the catalogue write mutex; only registration and event append use the narrow write critical section. A lifecycle journal remains through verified promotion and is removed only after catalogue persistence.

The local job history retains every job that is active, interrupted, failed, cancelled, has unfinished files, has pending metadata, or carries recovery evidence. Only fully completed jobs with no pending writes or unresolved evidence are automatically bounded to the newest 100. A future user-authorized discard operation may remove recoverable jobs explicitly; automatic pruning never does so. Copy progress persistence is throttled to 8 MiB checkpoints. User-facing ingest failures include a recovery action while retaining the underlying technical error.

Startup does not assume removable sources are present: queued, copying and verifying work becomes interrupted and waits for explicit retry. Recovery verifies promoted path, size, checksum and catalogue identity before registration, never removes a promoted destination and preserves ambiguous state for review. Completed files are checkpoints and are not copied again; an incomplete file restarts from byte zero because byte-offset resume is not implemented. Provisional metadata stays local until verified registration, then enters the authoritative event stream. This ingest job manager is the foundation for future enrichment, consolidation and cleanup jobs; those job types are not implemented.

Provisional metadata records which fields the user explicitly edited. An exact duplicate with untouched defaults reuses the existing media ID without emitting replacement/removal events. Completed-file edits first enter a durable pending slot in the local job record; successful authoritative append clears it, while startup retries unresolved writes against the latest projection. Progress events are merged with pending UI field patches so transfer updates cannot revert text currently being saved.

Discovery traversal/hashing and enrichment FFprobe/FFmpeg processes run outside the catalogue write mutex. Before appending events, each operation reloads the projection and rejects registrations or enrichment results made stale by a concurrent ingest or metadata commit.

FFprobe and thumbnail enrichment run sequentially only for records missing required metadata or cached previews and cannot invalidate a verified copy. External discovery persists observations, waits for repeated evidence across at least 15 seconds, rechecks previously indexed files when size/timestamp evidence changes, and hashes only candidates requiring verification. It starts asynchronously for configured libraries, publishes progress, persists the last result locally and emits catalogue changes after safe registration. Full verification deliberately rehashes previously verified files; quick scans skip unchanged evidence. A native notification watcher remains pending.

## Metadata, search, and Syncthing

The synchronized Master Bin contains portable relative paths, stable IDs and device-partitioned append-only schema-2 event logs. Devices never write another device's stream. A deterministic topological replay respects device sequence and observed-vector edges, then applies stable ordering only to concurrent events. Integrity observations are retained by device. The legacy snapshot is retained for recovery but is no longer operational state.

```text
device A events ─┐
device B events ─┼─ Syncthing ─► validated replay ─► local projection
device C events ─┘                                  ├─ in-memory state (now)
                                                    └─ SQLite/FTS (planned)
```

SQLite/FTS is a disposable device-local search projection, never a synchronized live database. Rebuilding it reads validated authoritative events plus media availability. The open Library polls a cheap event-file signature and rebuilds after shared streams change; a separate cheap path check updates device-local availability when Syncthing materializes known media without creating catalogue events or running discovery. Media discovery remains a separate operation. Syncthing temporary and conflict files are detected and reported, not silently treated as authoritative. Its optional local REST API supplies read-only health hints; PIMP works without it. Detailed rules are in [SYNCTHING_DESIGN.md](SYNCTHING_DESIGN.md).

## Identity and project architecture

Registration media IDs are UUIDs independent of hashes, names and paths. Replay derives a deterministic alias map for offline registrations only when normalized Master Bin relative path and SHA-256 both agree, then applies historic metadata events and project references to the canonical ID. Content hash alone never merges different paths, and one path with different hashes produces a visible conflict. This projection-time policy preserves append-only streams and existing IDs instead of performing a destructive migration. Folder and project IDs are likewise path-independent. Portable records use `/`-separated relative paths. Device settings map Master Bin and Working Projects roots locally.

Project creation accepts only a folder canonically contained beneath the configured Working Projects root. Existing ancestors are resolved before any directory is created, so traversal, symlink and junction escapes fail before registration. Compatible existing manifests may be explicitly associated inside that root. Catalogue metadata commits precede project-manifest projection; failed manifest writes are recorded in a device-local retry store and retried on later updates and startup without duplicating authoritative events.

Project-folder inventory is a read-only native adapter operation keyed by known project ID. It re-resolves the device-local authorized folder, walks without following links, records relative paths and basic filesystem metadata, and categorizes assets without hashing or parsing content. Reveal and open operations accept only that project ID plus a validated relative asset path. Premiere auto-save locations are excluded from deterministic main-project selection.

Media contains `projectIds[]`, so assignments are many-to-many. Each working directory owns an atomically updated `.pimp-project.json` manifest containing portable media identities, paths, checksums, and relevant metadata without recreating or deleting existing assets. See [DATA_MODEL.md](DATA_MODEL.md).

## Optional AI boundary

Full-edition analysis runs behind an optional process-isolated `AiProcessor` contract. A worker streams timestamped results with model ID/version, confidence where meaningful, cache identity, and provenance. Jobs require pause/resume/cancel, resource limits, retry policy, and failure isolation. Light may display compatible existing results but imports and ships no inference runtime or model. Model output never silently becomes manual fact.

## Premiere and archive boundaries

The standalone Premiere Companion UXP proof of concept uses documented active-project, import, folder/bin and media-path APIs. For its first boundary, PIMP creates a versioned, user-selected, no-overwrite interchange manifest containing only the explicit physical selection and validated canonical Master Bin paths. This avoids a persistent localhost service or network permission. The extension reconciles Premiere's observed media paths and never edits `.prproj` internals. Real Premiere/UXP acceptance remains open.

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

Device-local desktop preferences include startup page, launch-at-login, close-to-tray, theme, density and update-check intent. Tauri owns notification-area lifecycle: closing hides only when both the preference and a successfully created tray icon are present, preventing an inaccessible background process. Normal startup, second-instance activation, a tray click and Tray Show use one show/unminimize/focus routine; a rejected focus request is logged and requests taskbar attention because Windows may restrict foreground activation. Quit uses the application exit path; durable jobs remain recoverable. Release Windows binaries select the GUI PE subsystem and apply `CREATE_NO_WINDOW` only to FFmpeg/FFprobe children; debug builds keep console behavior. External support destinations are mapped by name to a fixed native HTTPS allowlist rather than accepting a URL from the webview.
