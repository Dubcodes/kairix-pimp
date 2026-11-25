# Product workflow and requirements register

This register captures the owner-confirmed lifecycle direction supplied on 2026-11-25. It distinguishes desired behavior from implemented code and from real-world acceptance so later milestones do not require the owner to restate the workflow.

## Owner-confirmed requirements

1. PIMP manages the lifecycle from removable-media ingest through catalogue use, editing-project evidence, completion, backup, optional cleanup and restoration.
2. Inventory is metadata-first and responsive. It shows filenames, sizes, totals, progress and source-device evidence without mandatory hashing or FFprobe work.
3. Ingest offers Fast and Deep verification. Deep remains the compatibility default. Both retain the copy-stream source SHA-256 unless measured evidence justifies a separately designed provisional identity.
4. Verification evidence states what actually occurred. Fast is not independent destination-content proof. Selective later Deep verification preserves stable identity and metadata.
5. SD-card cleanup is optional, job-specific and available in principle after either mode, but occurs only after whole-batch reconciliation in a durable native safety state machine. Development never targets production media.
6. Existing trustworthy duplicates may satisfy ingest without another Master Bin copy. Filename/size alone, catalogue-only records, missing copies and other-device-only evidence never authorize cleanup.
7. Associated recordings stay grouped; excluded/unknown camera support files and directories are not automatically removed.
8. Temporarily absent Master Bin media remains catalogued. Returning unchanged media keeps stable IDs, metadata, project relationships and compatible derivatives without blanket verification/reprocessing.
9. A returned completed-project folder resolves its existing stable project identity from portable manifests and reconnects only changed/ambiguous assets. Restoration is idempotent and path-portable.
10. Saved-project evidence distinguishes unverified association, verified import and verified timeline use. Outbound drag is not proof of import.
11. Future backup includes optional Master Bin mirroring and incremental Working Project history; retention/scheduling/deletion timing remain unresolved.
12. Light and Full share catalogue/project/analysis formats. Full's future local AI preserves reusable intermediate evidence; AI implementation is outside this milestone.

## Existing implemented functionality

- Metadata-only cancellable source inventory, approved immutable plan revisions, planned destinations and source-volume evidence.
- Copy-stream SHA-256, same-filesystem staging, source rechecks, atomic no-overwrite promotion, durable jobs/journals and global verified duplicate detection. Deep independently rereads staging; Fast records that it did not.
- Stable media/project IDs, append-only portable catalogue events, device-local availability, media relocation, shared thumbnails/scrub previews and selective integrity verification.
- Read-only saved-Premiere indexing, portable project manifests, verified external-drive project backup/restore testing and manual public-update discovery.

These are implementation claims backed by automated tests; installed production-scale and physical-device evidence remains separately stated in `TESTING.md`.

## Current milestone implementation status

- Requirements/ADR reconciliation: implemented in documentation.
- Fast/Deep verification and method evidence: implemented and automated-test covered; Deep remains the compatibility default. Fast evidence stays pending until selective Deep verification.
- Cleanup authorization/state machine: job-specific revisioned authorization, complete-batch reconciliation, consolidated review with validated Explorer actions, identity-bound evidence reuse, durable intent/outcome evidence and restart observation are implemented and automated-test covered. Ordinary-package removal is hard-disabled. A separately identified isolated acceptance build is packaged for exact generated fixtures only; owner-operated physical acceptance is pending.
- Temporary absence/return regression coverage: ordinary availability refresh proves disappearance and same-path return without deleting or duplicating the stable catalogue record. Different-path return continues through explicit relocation review; there is no destructive offload operation.
- Completed-project restoration foundation: a compatible `.pimp-project.json` may rebind an existing stable project only when its prior authorized local folder is unavailable. A live conflicting folder or mismatched identity blocks. Incremental saved-project/media reindexing beyond existing fingerprinted projections remains staged.

## Future planned functionality

- **Mark as completed**, supported Premiere collection detection, dependency verification and project finalization.
- Protection of footage used by unfinished projects and optional removal of verified redundant Master Bin copies/empty directories.
- PIMP-managed collection, offload/restore, Master Bin mirror, incremental backup history and supported online backup providers.
- Full-edition transcription, summaries, visual/audio observations, searchable evidence and storyteller workflows.

## Unresolved owner decisions

- Whether a temporary Explorer offload should propagate removal to other Syncthing devices.
- Default verification mode for new installations after performance and disposable-card acceptance.
- User-facing terminology for unused, associated, imported and timeline-used media.
- Backup retention, schedule, history depth and deletion timing.
- AI scheduling, approval workflow, storyteller output, model selection and worker installation.

## Acceptance evidence boundaries

- Automated tests use synthetic disposable data and prove code paths, serialization, failure handling and idempotence.
- Repository-built or installer-installed Windows exercises are reported separately and never inferred from jsdom/unit tests.
- Physical removable-media cleanup, disconnect/power-loss behavior, two-device Syncthing arrival and real Premiere host behavior require external acceptance.
- The owner's reported 1,030 physical items, 559 logical recordings, seven jobs and 28 pending verifications are production context only; development must not modify those files or catalogue records.
