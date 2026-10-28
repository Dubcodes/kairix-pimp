# Development roadmap

## Milestone 1 — foundation (complete)

Implemented and automated-test covered: domain types, date selection/fallback, folder metadata suggestions, deterministic event replay/deduplication, conflict surfacing, combined text search, path traversal rejection, atomic settings/manifests, safe file ingest, SHA-256 verification, collisions, duplicates, cancellation cleanup, malformed-event reporting, and project manifest preservation.

Implemented and tested with generated media: FFprobe metadata extraction and FFmpeg thumbnail generation for H.264 and H.265/HEVC fixtures.

UI implemented and component-test covered: application shell, primary navigation, library grid, folder/date presentation, search, multi-selection without Shift text highlighting, a reduced-motion-aware content scroll control, Light status, and the two required date mode names. Thumbnail placeholders and the responsive batch editor follow theme tokens. Batch tagging and many-to-many project assignment use the shared accessible searchable combo box with stable IDs and explicit tag creation.

## Milestone 2 — production ingest and advanced library (in progress)

- Implemented: native picker/drop, recursive inventory, verified copies, persisted real Library, per-ingest date overrides, explicit collision rename, projects, manual/periodic discovery, metadata/thumbnails and conservative same-stem MP4/WAV pairing
- Implemented reliability foundation: canonical causal event replay, deterministic path-plus-hash registration aliases, duplicate/malformed/newer-record handling, legacy migration/backup, multi-device convergence tests, external staging, minimal recovery journal, no-overwrite promotion, source-change checks, conservative discovery, two-sided unambiguous folder-aware pairing, portable thumbnails, Working Projects containment and recoverable manifest projection
- Implemented manifest reliability correction: serialized device-local reconciliation, pre-append retry intent, restart repair for a registered project's local folder mapping, failure-safe project creation ordering and no-op suppression for unchanged state/manifests
- Implemented project recovery correction: folder-first identity reuse across pending/settings/manifest evidence, explicit synchronization retry, conflict rejection and stale media-alias reconciliation after catalogue convergence
- Implemented Projects workspace: navigable project details, read-only categorized folder inventory, local status/size/main-project evidence, portable notes/tags, linked logical recordings and Library/Explorer navigation
- Implemented Projects workspace resilience: stale-request suppression, bounded read-only inventory, independent accessible row controls, grouped recording metadata actions and focus-preserving Library location
- Implemented Library usability: persistent icon-only sidebar, six deterministic folder sort modes plus accessible custom order, logical video/WAV metadata and counts, clean thumbnail placeholders, themed batch editing and content scroll-to-top
- Implemented: persistent device-local ingest jobs, bounded background worker, startup reconciliation, between-file pause, cancellation/retry, per-file job inspection and ingest-time tags/projects/notes/favourites
- Byte-range ingest resume and throughput history
- Native notification watcher, bounded thumbnail eviction, alternate thumbnail frames, folder notes and field-level conflict resolution workflow
- SQLite/FTS disposable projection, saved compound filters, pagination and measured large-library virtualization
- Implemented: lazy silent hover-to-scrub frames with bounded FFmpeg generation and a fingerprinted device-local cache; richer preview player, contact-sheet optimization and custom thumbnails remain planned
- Evidence-scored and reversible DJI camera/audio pairing
- Implemented: configurable date/time folder naming with live preview, requested `yyyymmdd` new-install default, separators, 12-/24-hour patterns, validation, and legacy preference preservation
- Remaining rich Library interaction: saved filters, richer previews and explicit review/conflict-resolution flows

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

Implemented for internal Light beta testing: repeatable Windows CI builds of unsigned MSI and NSIS installers, artifact identity metadata, prerelease publishing from versioned beta tags, a console-free release GUI subsystem, release-only media-tool console suppression, centralized tray/single-instance restore behavior and Windows Shell file selection. Remaining release work: complete installed console/focus/tray/Explorer acceptance, verify binary dependency contents, complete schema-2 and two-device Syncthing acceptance, sign, test upgrade/rollback, split future Full packaging, then validate Linux WebKit/FFmpeg packaging and macOS signing/notarization and sandbox behavior.
