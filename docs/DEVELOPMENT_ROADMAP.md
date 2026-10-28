# Development roadmap

## Milestone 1 — foundation (complete)

Implemented and automated-test covered: domain types, date selection/fallback, folder metadata suggestions, deterministic event replay/deduplication, conflict surfacing, combined text search, path traversal rejection, atomic settings/manifests, safe file ingest, SHA-256 verification, collisions, duplicates, cancellation cleanup, malformed-event reporting, and project manifest preservation.

Implemented and tested with generated media: FFprobe metadata extraction and FFmpeg thumbnail generation for H.264 and H.265/HEVC fixtures.

UI implemented and component-test covered: application shell, primary navigation, library grid, folder/date presentation, search, multi-selection, Light status, and the two required date mode names. Batch tagging and many-to-many project assignment use the shared accessible searchable combo box with stable IDs and explicit tag creation.

## Milestone 2 — production ingest and advanced library (in progress)

- Implemented: native picker/drop, recursive inventory, verified copies, persisted real Library, per-ingest date overrides, explicit collision rename, projects, manual/periodic discovery, metadata/thumbnails and conservative same-stem MP4/WAV pairing
- Implemented reliability foundation: canonical causal event replay, duplicate/malformed/newer-record handling, legacy migration/backup, multi-device convergence tests, external staging, minimal recovery journal, no-overwrite promotion, source-change checks, conservative discovery, folder-aware pairing, portable thumbnails and manifest identity protection
- Implemented: persistent device-local ingest jobs, bounded background worker, startup reconciliation, between-file pause, cancellation/retry, per-file job inspection and ingest-time tags/projects/notes/favourites
- Byte-range ingest resume and throughput history
- Native notification watcher, bounded thumbnail eviction, alternate thumbnail frames, folder notes and field-level conflict resolution workflow
- SQLite/FTS disposable projection, saved compound filters, pagination and measured large-library virtualization
- Contact sheets, preview player, custom thumbnails, folder rename/move journal
- Evidence-scored and reversible DJI camera/audio pairing
- Implemented: configurable date/time folder naming with live preview, requested `yyyymmdd` new-install default, separators, 12-/24-hour patterns, validation, and legacy preference preservation
- Rich library interaction: keyboard/multi-select batch actions, favourites, saved filters, previews and explicit review/conflict flows

## Milestone 3 — Premiere extension

Build and test a UXP extension against a supported Premiere version. Use active project access, `Project.importFiles`, transactional bin creation via `FolderItem`, and `ClipProjectItem.getMediaFilePath` to reconcile actual imports. Never edit `.prproj` internals.

Keep direct DCM Master Bin references, dated bins and an optional virtual Audio grouping. Project collection is an explicit verified transition: the collected `.prproj` in DCM Content becomes authoritative for the existing stable project ID only after dependency checks; presence alone never changes state.

## Milestone 4 — Full AI

Add a separately packaged worker protocol, queue, resource governor, cache, and versioned results. Evaluate whisper.cpp for compact cross-platform distribution and faster-whisper where CUDA throughput justifies Python/CTranslate2 packaging. Add frame-sampled vision and audio-event models only after benchmark and licensing review. No model is bundled in Light.

## Milestone 5 — verified consolidation/archive

Inventory dependencies, support complete-source archives first, integrate Premiere Project Manager and/or PlumePack through supported interfaces, verify the output project, then add opt-in transcode/trim modes with handles and provenance. Transcodes receive new hashes linked by derivation records.

Retain lightweight completed-project records in catalogue search and a portable project-local archive carrying stable project identity and its media manifest. Public sharing links and YouTube metadata remain deferred.

## Milestone 6 — safe cleanup

Unused-footage review, cross-project protection, synchronization preflight, recoverable quarantine, multi-step explicit authorization, and delayed purge. No direct deletion from source cards.

Automation is opt-in and disabled by default. Per-file protection and successful finalization verification gate eligibility; Syncthing replicas are not treated as backups.

## Cross-cutting — persistent background work

The versioned background job foundation is implemented for ingest: it persists intent and completed-file checkpoints, distinguishes incomplete from committed records, and reconciles safely at startup. Extend the same bounded, device-local pattern to enrichment, consolidation and cleanup without treating those future job types as implemented.

## Milestone 7 — packaging and additional platforms

Produce separate Windows Light and Full installers from the same codebase, verify binary dependency contents, sign, test upgrade/rollback, then validate Linux WebKit/FFmpeg packaging and macOS signing/notarization and sandbox behavior.
