# Development roadmap

## Milestone 1 — foundation (current)

Implemented and automated-test covered: domain types, date selection/fallback, folder metadata suggestions, deterministic event replay/deduplication, conflict surfacing, combined text search, path traversal rejection, atomic settings/manifests, safe file ingest, SHA-256 verification, collisions, duplicates, cancellation cleanup, malformed-event reporting, and project manifest preservation.

Implemented and tested with generated media: FFprobe metadata extraction and FFmpeg thumbnail generation for H.264 and H.265/HEVC fixtures.

UI implemented: application shell, primary navigation, library grid, folder/date presentation, search, multi-selection, batch action affordances, Light status, and the two required date mode names. The displayed media is clearly development fixture data. Native filesystem/drag-drop wiring is not complete.

Scaffolded: Tauri 2 application and minimal capability policy. Native build is unverified due to missing Rust/MSVC tools.

## Milestone 2 — production ingest and advanced library

- Native folder picker/drop events and recursive directory ingest preserving camera structures
- Durable resumable job journal, pause/cancel, throughput and per-file progress
- Media fixtures for H.264/H.265 probe and thumbnail regression tests
- SQLite/FTS disposable projection, saved compound filters, pagination/virtualization
- Contact sheets, preview player, custom thumbnails, folder rename/move journal
- Evidence-scored and reversible DJI camera/audio pairing

## Milestone 3 — Premiere extension

Build and test a UXP extension against a supported Premiere version. Use active project access, `Project.importFiles`, transactional bin creation via `FolderItem`, and `ClipProjectItem.getMediaFilePath` to reconcile actual imports. Never edit `.prproj` internals.

## Milestone 4 — Full AI

Add a separately packaged worker protocol, queue, resource governor, cache, and versioned results. Evaluate whisper.cpp for compact cross-platform distribution and faster-whisper where CUDA throughput justifies Python/CTranslate2 packaging. Add frame-sampled vision and audio-event models only after benchmark and licensing review. No model is bundled in Light.

## Milestone 5 — verified consolidation/archive

Inventory dependencies, support complete-source archives first, integrate Premiere Project Manager and/or PlumePack through supported interfaces, verify the output project, then add opt-in transcode/trim modes with handles and provenance. Transcodes receive new hashes linked by derivation records.

## Milestone 6 — safe cleanup

Unused-footage review, cross-project protection, synchronization preflight, recoverable quarantine, multi-step explicit authorization, and delayed purge. No direct deletion from source cards.

## Milestone 7 — packaging and additional platforms

Produce separate Windows Light and Full installers from the same codebase, verify binary dependency contents, sign, test upgrade/rollback, then validate Linux WebKit/FFmpeg packaging and macOS signing/notarization and sandbox behavior.
