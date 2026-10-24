# Project brief

## Vision

Kairix PIMP — Production Ingest & Media Processor — is a practical desktop companion to Syncthing for the complete production-media lifecycle: verified camera offload, organization, catalogue search, project use, and eventually verified archive and safe cleanup. It is designed for editors who need footage to remain trustworthy while moving between computers and Adobe Premiere Pro projects.

PIMP does not replace Syncthing. A synchronized **Master Bin** holds original media and portable authoritative metadata. **Working Projects** hold active Premiere projects, project manifests, music, graphics, images, and other editing assets. Each computer maps these locations to its own absolute paths; synchronized references use stable identities and relative paths.

## Intended workflow

1. Connect camera storage and drag files or folders into PIMP.
2. Choose exactly **Use today's date** or **Use original file dates** for folder organization.
3. Copy through temporary destinations, verify independent checksums, preserve source media, then catalogue successful copies.
4. Pair related video/audio only where filename, time, duration, and camera evidence are sufficient; keep pairing reversible.
5. Organize folders, apply folder-level and clip-level tags, and browse previews/contact sheets.
6. Combine search filters across names, dates, cameras, technical fields, tags, projects, review state, transcripts, and other metadata.
7. Assign a media item to zero, one, or many projects while preserving stable IDs.
8. Optionally run local Full-edition transcription, visual, and audio analysis. Store model provenance and uncertainty separately from manual metadata.
9. Import into Premiere through a documented UXP extension, retain dated bin structure, and reconcile the actual project state.
10. Track active projects through finalization. Inventory dependencies, produce a self-contained archive, verify independence and integrity, protect media used elsewhere, then offer explicitly authorized recoverable cleanup.

Windows is the first release target. Portable records and adapters are designed for future Linux and macOS implementations, which remain unverified.

## Editions

- **PIMP Light:** the complete non-AI workflow, including ingest, catalogue/search, project and Premiere workflows, archive workflows, and reading compatible AI metadata generated elsewhere. It must contain no unnecessary AI models or inference dependencies.
- **PIMP Full:** the same application, catalogue, and project formats plus optional local workers for transcription, timestamped search, scene/activity/object/sound analysis, and suggested tags.

AI processing is optional, local, modular, cancellable, and unable to block ordinary ingest or browsing.

## Current implementation — October 2026

Implemented now: the tested portable core and Node reference adapter plus a compiled Tauri host for device settings, restricted file and folder selection, native drops, recursive verified ingest, authoritative per-device event streams, project manifests, conservative discovery, FFprobe metadata and FFmpeg thumbnails. Ingest runs through durable device-local jobs with stable IDs, bounded single-worker execution, per-file progress, between-file pause, cancellation and explicit retry. The real Library remains usable during transfer and supports folder browsing, combined search, multi-selection, convergent tags and multiple project assignments.

Normal operation contains no synthetic catalogue entries. New devices show setup and empty-library guidance. A 30-second reconciliation scan plus manual Rescan tracks candidates across repeated observations, reports pending files and revalidates changed known paths. Separate lightweight polling refreshes shared metadata and device-local availability without a discovery scan. Native ingest stages outside Master Bin; startup recovery marks unfinished work interrupted, independently verifies promoted bytes, reconciles completed registrations and preserves ambiguous journals for review. Provisional tags, projects, notes and favourites survive restart and retry and become authoritative only after verified registration. Byte-range transfer resume remains planned.

Schema-2 event replay, migration, simulated multi-device convergence, damaged-stream handling, no-overwrite promotion, discovery stability and project identity protection have direct Rust tests. Native compilation and unsigned MSI/NSIS packaging are automated on Windows. The Library exposes root-validated Windows Explorer and path-copy actions, reports local/synchronizing availability, and warns when optional FFmpeg/FFprobe features are unavailable without blocking ingest. A schema-2 installed native smoke pass remains required before limited real-media use. Release signing/upgrades, real Syncthing multi-device acceptance, folder notes, conflict resolution UI, preview-frame selection and bounded-cache eviction remain incomplete.

Planned, not functional: Premiere integration, Full-edition AI processing, verified consolidation/archive, destructive cleanup, signed release delivery, and Linux/macOS releases. See the [roadmap](DEVELOPMENT_ROADMAP.md) and [testing strategy](TESTING.md) for the exact boundaries.
