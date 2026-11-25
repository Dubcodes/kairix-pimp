# Controlled SD-card source cleanup design

The owner has authorized development of optional automatic SD-card cleanup after Fast or Deep ingest. That product authorization is not permission to delete production media during development. The current public beta remains copy-only until the state machine below has complete native failure-injection coverage and real installed disposable-removable-media acceptance.

Master Bin/project cleanup is a different lifecycle operation. It still requires verified project collection, archive independence, cross-project protection, recoverable quarantine and separate authorization.

## Implementation checkpoint

The durable ingest-job schema, job-specific revisioned authorization, whole-batch reconciliation, consolidated review, validated Explorer actions, locked authorization revision, per-file intent/outcome journal and restart observation are implemented with synthetic native tests. Persisted source/destination file identities, snapshots and volume identities let unchanged Fast or Deep evidence survive cleanup reconciliation without another content read. A changed destination timestamp triggers one targeted SHA-256 read; replacement or missing identity blocks. Final source removal uses an identity-bound exclusive handle and does not reread source content.

Ordinary Development and Stable builds stop at `acceptance-required`. The `cleanup-acceptance` Cargo feature is separately packaged under `com.kairix.pimp.cleanup-acceptance`. It creates an exact synthetic fixture on a selected empty detected removable volume, redirects settings/catalogue/cache roots to isolated application data, revalidates the fixture manifest before the same native removal path, rejects additions/substitutions/non-removable media and exports JSON evidence. It is not a production enablement switch.

## Content-pass policy

- Fast new copy: one source read while hashing/writing staging, one destination write and no destination reread. Cleanup adds metadata/identity checks only while evidence is unchanged.
- Deep new copy: the Fast work plus one independent staged-destination read. Cleanup adds no unchanged-content pass.
- Existing duplicate: one source hash and one independent current destination hash establish the duplicate; cleanup reuses that identity-bound evidence.
- A changed timestamp or discrepancy triggers a targeted affected-destination hash. Missing or replaced identities fail closed instead of falling back to path/size trust.

Targeted cleanup reads acquire destination-volume admission and are added to per-file cleanup diagnostics. Metadata and handle validation are not misreported as content bytes.

## Durable state model

Cleanup is a phase of one persistent ingest job, never a per-file side effect of copying. The job records:

- selected verification mode and actual per-file verification evidence;
- `cleanupRequested`, its monotonic authorization revision, change time and last accepted source;
- the transition that locks authorization when cleanup begins;
- approved source roots and source-volume identities;
- every approved file's canonical path, relative path, size, modified snapshot, copy-stream SHA-256, group identity and disposition;
- destination media ID/path, catalogue registration and Fast/Deep evidence;
- excluded files and camera support files that are never cleanup targets;
- per-file eligibility, deletion intent written before removal, observed result and post-attempt reconciliation;
- interruption, disconnect, replacement, retry and consolidated-review evidence.

Older jobs deserialize as Deep with cleanup disabled. A stale UI/settings write or restart can never turn cleanup on.

## Authorization lifecycle

The ingest UI exposes **Automatically delete ingested files from SD card** beside the ingest action. It is off by default. Its job-specific value remains visible and editable during copy, verification and reconciliation. Each change persists a higher authorization revision and produces one small old-to-new notice. The control locks only when native cleanup enters `cleanup-revalidating`; no global preference or frontend-only state authorizes removal.

Fast mode messaging states that the destination was not independently reread. No repeated per-file confirmation is required for routine success, but the irreversible removable-media consequence remains visible before ingest and while authorization is editable.

## Whole-batch reconciliation

Cleanup cannot begin until every approved file has an explicit terminal outcome and the native host reconciles individual files plus aggregate counts/bytes:

1. newly copied and catalogue-registered;
2. existing duplicates with trustworthy local destination evidence;
3. deliberately excluded support files retained on the source;
4. failed, cancelled, changed, unavailable or unresolved files;
5. expected and observed destination files/bytes;
6. source files added or changed after approval.

Equal totals are never sufficient. A discrepancy triggers targeted investigation, including Deep verification of affected source/destination content where necessary. Any unresolved discrepancy blocks the entire automatic cleanup phase and produces one actionable review. Successful files remain completed and retryable work does not restart them.

## Per-file and recording-group eligibility

Filename and size are candidate hints only. A newly copied Fast file may be eligible only when its exact source snapshot/volume, flushed promoted destination, source copy-stream hash, catalogue identity and unchanged source all agree. An existing duplicate requires a fresh trustworthy local destination; targeted source/destination hashing is required when current Deep evidence is absent. Missing or other-device-only copies are never sufficient.

Known associated video/audio members are protected as a group. Excluded thumbnails, unknown sidecars, executables, metadata and other support files remain untouched. The first implementation removes individual eligible recording files only; it never recursively removes or formats directories and never assumes DJI naming applies to other cameras.

## Native revalidation and exclusive cleanup

Before each removal the native host must:

1. load the durable intent and confirm cleanup is still authorized at the locked revision;
2. acquire exclusive source-volume cleanup admission and exclude ingest, scan, update and other cleanup work;
3. re-identify the same source volume, rejecting drive-letter reuse or substitution;
4. resolve the exact approved path canonically beneath the approved source root without symlink/junction/reparse escape;
5. confirm regular-file type, identity where available, size, modified snapshot and required hash evidence;
6. revalidate the promoted destination's containment, availability, catalogue identity and required verification evidence;
7. write and flush the per-file deletion intent before the filesystem operation;
8. remove only that file, then record whether it is absent, still present, replaced or indeterminate before advancing.

If the process stops after intent but before outcome, restart performs observation and reconciliation. It never blindly repeats removal. A replaced, reconnected or modified card blocks remaining cleanup.

Recoverable same-filesystem quarantine is preferred where practical. Many camera cards cannot provide a meaningful recoverable quarantine; irreversible removal therefore stays production-disabled until installed disposable-card acceptance proves the exact path/volume/restart behavior and UI warning.

## Required acceptance before production enablement

Automated disposable fixtures must cover cleanup on/off, authorization changes during copy/verification, stale revisions, complete and discrepant batches, new copies, existing duplicates, same-name/size changed content, excluded support files, paired recordings, source mutation/substitution/disconnection, permission/disk errors, interruption around every intent transition and restart without repeated removal.

Installed Windows acceptance must use a disposable removable volume, review the complete proposed/retained set, exercise Fast and Deep batches, disconnect/reconnect and terminate during cleanup, then prove only intended synthetic files were removed. Until that passes, code may expose review/scaffolding but production cleanup execution remains unavailable.

The isolated acceptance build records the generated fixture manifest and canonical source root independently in its own application-data directory. Validation requires the removable-media manifest to match that trusted record byte-for-byte in its structured fields, the selected job to contain exactly those fixture files, the original removable-volume identity to remain present, and the destination to resolve to the acceptance application's own isolated workspace. A card-side manifest cannot define its own deletion boundary. Post-removal observation treats only an explicit not-found result as confirmed absence; permission, device, replacement and other inspection errors remain indeterminate with durable intent evidence. Ordinary builds still cannot execute source removal.
