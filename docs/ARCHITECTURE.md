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
- Library selection presentation remains device-local: the floating-toolbar layout uses WebView local storage, and the bounded undo/redo stack holds stable media IDs plus the range anchor in memory only. Neither writes presentation history to portable events.
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

FFprobe and thumbnail enrichment run sequentially only for records missing required metadata or cached previews and cannot invalidate a verified copy. External discovery persists observations, waits for repeated evidence across at least 15 seconds, rechecks previously indexed files when size/timestamp evidence changes, and hashes only candidates requiring verification. The filesystem walk and hashing execute on a blocking worker, stream accepted entries instead of retaining a complete `DirEntry` list, and publish bounded progress while the Library remains responsive. It starts asynchronously for configured libraries, persists the last result locally and emits catalogue changes after safe registration. Full verification deliberately rehashes previously verified files, but an observation identical to the current device's size, timestamp, SHA-256 and review state emits no authoritative event. The same equality check is repeated against the latest projection under the catalogue write lock to suppress a concurrent duplicate. Stable observations are retained until event persistence succeeds, so append failure is retryable without a new stability delay. Quick scans continue to skip unchanged evidence. A native notification watcher remains pending.

Static thumbnails and multi-frame hover previews use independent single-active frontend queues and independent native FFmpeg lanes. A running hover encode therefore cannot occupy the only scheduler slot needed by visible Library images, while each lane remains conservatively limited to one decoder. Static work retains bounded per-media deduplication and a shared native lane; FFmpeg extraction has a 60-second timeout so a malformed source cannot hold that lane indefinitely. Cache validation occurs before decoding; generation uses a cache-local temporary file, source-stability recheck and atomic promotion; catalogue writes remain a separate narrow phase. The frontend registry contains only in-flight promises. Failure state is separately bounded and backed off, and source/cache-generation keys prevent stale failures from poisoning a replacement identity. A batched catalogue-source refresh merges around URLs prepared after that batch began so navigation cannot replace a fresh URL with stale absence. Hover readiness is loaded once per catalogue revision from fingerprint-matched, validated cache manifests rather than checked during card rendering.

Native transient operation records expose the actual lifetime and stage of static-thumbnail and hover-preview workers through the same Background Operations projection as scans, enrichment, playback proxies, project indexing and backups. The frontend polls that device-local projection, selects one real primary operation for the native title and Windows taskbar state, and uses determinate progress only when that operation supplies a measured total. These high-frequency records are never catalogue events.

External-drive inspection is divided into metadata-only quick inventory, explicit full-content SHA-256 verification and optional deep analysis. Metadata equality never becomes content identity. The planned volume-scoped admission controller, priorities, measured-throughput rules and slow-drive backoff are defined in [Resource-aware media processing](RESOURCE_MANAGEMENT.md); destructive archive operations remain unavailable.

A priority-aware global derivative gate serializes FFmpeg across playback proxies, custom/static thumbnails and hover generation while retaining their separate request queues. Foreground playback and visible thumbnails outrank queued background enrichment/preparation after the current child exits. Playback proxies use two-second keyframes, `fastdecode` and `+faststart`; their versioned fingerprint prevents reuse of the older transform. Only a finalized, FFprobe-validated proxy is exposed.

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

The first provider-neutral backup adapter operates below future Premiere collection: it inventories an entire known Working Project, stages on a separately selected destination volume, promotes without overwrite and emits an immutable portable SHA-256 manifest only after a full destination reread. Mutable progress, absolute roots and destination-volume identity remain device-local in `backup-jobs.json`; active states become explicit interrupted work after restart. Verified generations can be rechecked and restored to a separate empty directory. This proves copied bytes, not Premiere dependency completeness or archive independence.

Cloud providers consume the same manifest boundary. Upload acknowledgement, a remote listing, provider-side size metadata, and a provider checksum with undocumented semantics remain verification-pending. Verification requires PIMP to independently read every remote object through a documented provider interface and compare its bytes or SHA-256 with the immutable local manifest. Provider authentication belongs behind a narrow adapter and operating-system credential storage; secrets, refresh tokens and browser sessions never enter portable manifests, synchronized metadata, command-line arguments or ordinary logs.

Degoo remains an explicit official-browser handoff because no supported unattended API was identified. The reviewed community client demonstrates a historic reverse-engineered web protocol, not an approved integration contract: it depends on private endpoints, observed client keys/headers and browser-fingerprint impersonation, and it does not provide reliable Windows operation or independent restored-byte verification. PIMP does not copy that client, call those endpoints, retain its authentication material or treat browser automation as an unattended backup adapter. Native Degoo work requires a documented supported interface or written provider authorization before implementation.

The standalone Premiere Companion UXP proof of concept uses documented active-project, import, folder/bin and media-path APIs. For its first boundary, PIMP creates a versioned, user-selected, no-overwrite interchange manifest containing only the explicit physical selection and validated canonical Master Bin paths. This avoids a persistent localhost service or network permission. The extension reconciles Premiere's observed media paths and never edits `.prproj` internals. Real Premiere/UXP acceptance remains open.

The offline fallback indexes only the deterministic main saved `.prproj` candidate from a stable read. It bounds compressed/decompressed bytes, XML events, nesting, values and graph traversal; rejects document types; and never interprets `.prin` payloads or writes project files. A narrow object graph resolves unambiguous `ObjectUID`/`ObjectID` definitions, `ObjectURef`/`ObjectRef` references, project-item containers, bins and clip track items. Only named canonical sequence definitions are exposed. Established container placement separates top-level from nested sequences, while resolvable track-item paths provide saved-timeline placement counts. Arbitrary paths remain unclassified; a path becomes historical only when the preceding device-local projection contains positive imported/used evidence for that exact path. Incomplete timeline coverage remains unknown rather than an inferred zero. Unresolved evidence stays visible and unverified time-base/range fields remain unknown. The fingerprinted JSON projection is device-local, and only exact existing canonical-path equality associates a reference with a catalogue ID.

The presentation layer gives editable controls and transient interaction surfaces ownership of keyboard input. Global playback, navigation and selection-history shortcuts run only after input, contenteditable, combo-box, menu and modal ownership checks, so native text editing and accessible component navigation remain intact.

Ordinary thumbnail references remain portable cache-relative evidence, never proof that device-local bytes exist. Startup enrichment performs a catalogue pass and regenerates only video records whose technical metadata or expected cache-owned JPEG is missing/invalid. In addition, a missing available video entering the near viewport requests one fingerprint-keyed, bounded visible-thumbnail operation so catalogue-order background work cannot leave the active scroll region waiting. Library and Projects consume the same parent-owned media-ID source map and never start page-specific enrichment. Hover preparation does not begin for a card until its static thumbnail is available. FFmpeg writes a unique cache-local temporary JPEG, PIMP validates source stability plus the bounded JPEG header/size, then atomically renames it into place. The WebView receives only validated cache paths through the Tauri asset protocol and falls back cleanly on load errors. Explorer reveal is a Windows adapter boundary: canonical paths are converted to compatible short drive/UNC syntax only for the Shell call, while the parent folder PIDL and relative child PIDL are passed separately.

The Clip Viewer stays in the existing WebView and resolves playback only through a known media ID and the native root-validation boundary. WebView2 owns ordinary media controls. Playback failure offers an explicit, on-demand FFmpeg fallback fingerprinted by media identity, approved SHA-256, source snapshot and transform parameters. Per-fingerprint coordination prevents duplicate encoders. FFmpeg reports actual processed timestamps through Background Operations and cancellation kills and waits for the process before deleting its partial. FFprobe must confirm a finalized nonempty MP4 with nonzero duration, H.264 yuv420 video and AAC audio where present; PIMP then rechecks the source and promotes without overwrite. A separate persisted proxy-cache allowance protects active playback and generation paths and evicts only ordinary cache-owned files without following links. Manual frame selection uses the same cache-only/source-stability boundary for JPEG output. These derivatives are device-local and never replace source or synchronized media.

An approved ingest plan stores the exact source selection, snapshots, optional source-volume identity, SHA-256 values, package-date evidence and destination mapping. A per-batch date change runs on a blocking worker and stats only those exact source files; it never walks the selected roots, discovers files or rereads source content. It then recomputes destinations and duplicate/collision state with the stored hashes. Immutable revisions share a lineage, remain valid until the replacement is durably written, and are serialized by both the UI intent queue and native plan store. Ingest rejects a superseded revision and consumes the lineage only after its durable job is created.

Projects uses the same canonical media IDs, logical MP4/WAV grouping and device-local thumbnail-source map as Library. Assigned media is rendered immediately. The unassigned catalogue projection stays unmounted until explicitly expanded, then applies local text/tag filters and renders at most 50 rows per page. This is a bounded presentation projection only; assignments still write normal authoritative media metadata events.

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

## Update-operation coordination

The updater's pending resource is device-local ephemeral state. Checks and installations are single-flight; every failed check clears stale state, and a failed download requires a fresh signed check. The native operation coordinator closes the authorization race in both directions: installation begins only when durable ingest/backup state and active critical-operation permits are safe, and after authorization new ingest lifecycle changes, backups/restores, scans/verifications, enrichment and preview filesystem work fail closed. Permits are RAII-scoped and the coordinator lock is never held during long work. Tauri signature validation remains the trust boundary; this coordination protects recoverability but does not claim an installed upgrade has passed.

Source review now produces a persisted device-local approved ingest plan containing the exact source paths, source-volume identity, snapshots, hashes, destination mapping and duplicate disposition. Inventory traversal, chunked hashing and duplicate inspection execute in one serialized blocking worker rather than on the Tauri command/event thread. Requests carry an operation ID, publish stage/item/byte progress, cooperatively cancel between directory entries and hash chunks, and supersede stale requests before they can publish a plan. Job creation consumes that plan without rereading every source hash. The copy worker hashes the bytes it actually reads and must match the approved hash before promotion; it also revalidates the source volume before and after the transfer. MP4/WAV SD-card scans use the same verified ingest worker with a narrower configurable filter. Global duplicate lookup uses catalogue SHA-256/size only to find candidates; it then resolves a candidate under Master Bin and freshly verifies snapshot, size and SHA-256 before a no-copy completion. Catalogue-only or failed verification is never trusted. Active jobs also reserve content hashes so concurrent jobs cannot independently copy the same bytes unless the user explicitly requests a second physical copy.

The UI's Background Operations view combines typed inventory, discovery/integrity, static-thumbnail enrichment, playback-proxy, project-backup, ingest and saved-Premiere-index status without creating a second authoritative job store. Each operation reports real file/byte/time counters when available and otherwise exposes its actual stage with an indeterminate progress bar; high-frequency status remains device-local and never becomes a catalogue event. Inventory plans with colliding destinations remain review-only until the user supplies safe replacement filenames and PIMP persists a new approved plan. A missing or corrupt prior catalogue copy can be reimported only through an explicit per-start choice; this does not weaken source-change rejection.

A future shared Resource Management scheduler may coordinate CPU, decoder, RAM, temporary-disk and battery policy across these typed operations and optional Full workers. It must preserve the current priorities—foreground static thumbnails and explicit playback above automatic hover preparation, verified ingest/integrity safety above optional derivatives—and use the existing operation/job contracts rather than replacing durable ingest/backup state with transient UI status. This milestone adds observability and bounded per-subsystem queues only; it does not claim a unified governor.

Newly registered media requests one coalesced background enrichment pass. FFmpeg/FFprobe failure never blocks ingest and produces no no-op enrichment event. Ordinary thumbnail JPEGs are evicted oldest-first under the configured device-local byte limit; hover-preview storage remains separately governed.

## Security boundaries

- Tauri capabilities expose only required commands to approved windows.
- Canonicalize and validate filesystem roots, symlinks/junctions, reserved names, Unicode, and case collisions.
- Invoke processes without shell interpolation; constrain executable discovery and arguments.
- Store a future Syncthing API key in the OS credential store and never in synchronized metadata/logs.
- Store any future provider access/refresh tokens in the OS credential store; never accept credentials through process arguments or persist browser cookies/sessions as application integration state.
- Use only documented provider interfaces or explicit written authorization, and fail cloud verification closed until independently restored bytes match the portable manifest.
- Treat media metadata, AI output, event files, Premiere data, and external tool output as untrusted input.
- Destructive actions require distinct review and explicit user authorization; none exist in the current milestone.

## Cross-platform considerations

Windows/WebView2 is first. Core records preserve canonical spelling while detecting case-fold hazards, never store drive letters as portable identity, and isolate file watching, removable-device discovery, process invocation, credential storage, and path rules behind adapters. Linux WebKit/FFmpeg packaging and macOS signing/notarization/sandbox behavior need later platform-specific implementations and tests; portability in design is not a verified release claim.

Device-local desktop preferences include startup page, launch-at-login, close-to-tray, theme, density and update-check intent. Tauri owns notification-area lifecycle: closing hides only when both the preference and a successfully created tray icon are present, preventing an inaccessible background process. Normal startup, second-instance activation, a tray click and Tray Show use one show/unminimize/focus routine; a rejected focus request is logged and requests taskbar attention because Windows may restrict foreground activation. Quit uses the application exit path; durable jobs remain recoverable. Release Windows binaries select the GUI PE subsystem and apply `CREATE_NO_WINDOW` only to FFmpeg/FFprobe children; debug builds keep console behavior. External support destinations are mapped by name to a fixed native HTTPS allowlist rather than accepting a URL from the webview.
