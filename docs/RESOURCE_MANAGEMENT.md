# Resource-aware media processing

This design governs future external-drive inventory, verification and deep analysis. It does not authorize cleanup, archive mutation or automatic source deletion.

## Inspection levels

### Quick inventory

Quick inventory reads directory entries and file metadata: canonical relative path, filename, size, modification time, volume identity where available, and already-cached technical evidence. It does not open complete media content, calculate a new SHA-256, start FFmpeg/FFprobe merely to populate the listing, or treat metadata equality as content identity.

An unchanged quick observation may reuse prior verified evidence for display, but the UI must label the evidence age and strength. A path/size/time match is only a candidate match.

### Content verification

Content verification reads every byte and calculates SHA-256 only when verified ingest requires it, a possible duplicate must become a content-identity decision, the user explicitly requests integrity verification, or a safety-critical transition lacks current evidence. Verification records the volume and file snapshot before and after the read. Changed, disconnected, unreadable or throttled sources remain unresolved and retryable.

Verified evidence is not cleanup authorization. Source cleanup additionally requires the complete state machine in `SOURCE_CLEANUP_DESIGN.md`, exact review and explicit confirmation.

### Deep analysis

Deep analysis covers FFprobe, thumbnail/preview generation and future optional local AI. It is derivative, cancellable and lower priority than ingest, independent verification, playback and explicit foreground preview. Light never imports an AI runtime. Analysis failure cannot invalidate verified original media.

## Device-local resource policy

Each observed volume has device-local activity and performance evidence. The implemented volume governor prefers the Windows volume GUID, falls back conservatively where unavailable, atomically sorts/merges source and destination needs, and admits one full-content reader or writer per volume. This prevents deadlock and drive-letter reuse from silently sharing prior identity. It coordinates approved source hashing, verified ingest copy/rechecks, explicit Library verification, project backup/restore verification, playback proxies, thumbnails, hover previews and enrichment. Separate logical volumes that may share one physical device remain conservatively disclosed because Windows does not always provide a reliable grouping relationship.

The existing derivative gate still permits one FFmpeg process globally and orders waiting work as foreground playback/custom frames, visible static thumbnails, interactive hover work, then background hover/enrichment. Resource profiles affect actual background admission across independent volumes: Quiet and active Balanced admit one background worker, active Performance admits two, and a safely eligible idle boost permits one additional worker. Gentle scanning adds admission spacing; pause holds automatic work at the next file/process boundary while explicit ingest, verification, playback and visible thumbnails remain available. Before starting work, the implemented governor considers:

- active ingest, restore, backup or integrity verification;
- explicit foreground playback/preview activity;
- whether another worker is reading the same volume;
- recent throughput, read latency and retry/error rate;
- user pause and profile/gentle admission spacing;
- supported Windows input-idle, CPU-load and available-memory evidence before idle expansion.

Explicit per-volume byte-rate caps, reserved temporary/cache space, GPU load and battery policy are not yet enforced. They remain visible limitations rather than settings that only appear to work.

Slow or failing volumes back off exponentially with a bounded delay after reported I/O failures. The worker never spins on a disconnected drive. It resumes only from durable evidence and revalidates volume/file identity. Sequential reads are preferred; concurrent full-file hashing of one identified volume is prohibited.

CPU and RAM readings are measurements only when supplied by a supported host API. Disk throughput is calculated from bytes actually read over elapsed monotonic time. Full-content hashing/copy verification reports known streamed bytes. FFmpeg does not expose source bytes read through the current integration, so thumbnails, hover previews and playback proxies report completed output bytes but do not add the source's logical file length to measured-read counters. PIMP must label heuristics as estimates and must not infer physical-device saturation from queue length alone.

## Scheduling priorities

1. Verified ingest copy/verification and explicit restore verification.
2. Foreground playback and user-requested preview/thumbnail work.
3. Visible and near-visible static thumbnails.
4. Explicit integrity checks.
5. Hover-preview preparation and enrichment.
6. Optional idle deep analysis.

Separate derivative lanes do not grant unlimited I/O. The global FFmpeg gate and volume permits are both required. Priority is applied only at safe admission boundaries; active copies, promotions and child processes are not preempted. Queue aging advances lower-priority work after bounded completed admissions. Cancellation while waiting removes the request, shutdown wakes waiters, and RAII release prevents abandoned volume ownership.

Quick Library inventory enumerates paths, sizes and timestamps only. It retains candidates as pending and never silently hashes or registers them. Explicit full verification performs SHA-256 under a volume permit. Approved ingest inventory remains an explicit content-verification workflow because the reviewed hash is required before safe copy.

## Required evidence and acceptance

Automated synthetic tests cover per-volume serialization, independent volumes, reused drive-letter identities, atomic source/destination acquisition, foreground ordering, profile/idle transitions, cancellation, shutdown, bounded error backoff and measured byte counters. Installed acceptance must still use disposable fast and deliberately slow external media under concurrent ingest/playback, disconnect/reconnect and safe pause/resume. Record drive type, connection, corpus, elapsed time, throughput, process count, CPU/RAM and worker states. No production card or Master Bin deletion is part of this acceptance.
