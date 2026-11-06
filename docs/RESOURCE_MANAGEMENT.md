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

Each attached volume will have a device-local policy and observed performance history. The initial implemented derivative admission gate permits one FFmpeg derivative process globally and orders waiting work as foreground playback/custom frames, visible static thumbnails, interactive hover work, then background hover/enrichment. The static and hover queues remain logically independent, but they do not run simultaneous high-bandwidth decoders. Before starting a derivative read, the future volume-aware governor must additionally consider:

- active ingest, restore, backup or integrity verification;
- explicit foreground playback/preview activity;
- whether another worker is reading the same volume;
- recent throughput, read latency and retry/error rate;
- user pause and per-volume throughput allowance;
- free temporary/cache space and system memory pressure.

Slow or failing volumes back off exponentially at file boundaries. The worker never spins on a disconnected drive. It resumes only from durable evidence and revalidates volume/file identity. Sequential reads are preferred; concurrent full-file hashing of one physical drive is prohibited by default.

CPU and RAM readings are measurements only when supplied by a supported host API. Disk throughput is calculated from bytes actually read over elapsed monotonic time. PIMP must label heuristics as estimates and must not infer physical-device saturation from queue length alone.

## Scheduling priorities

1. Verified ingest copy/verification and explicit restore verification.
2. Foreground playback and user-requested preview/thumbnail work.
3. Visible and near-visible static thumbnails.
4. Explicit integrity checks.
5. Hover-preview preparation and enrichment.
6. Optional idle deep analysis.

Separate derivative lanes do not grant unlimited I/O. The shared global admission controller now serializes FFmpeg and prioritizes foreground waiters after the active process exits safely. A future extension will issue volume-scoped read permits, incorporate ingest/copy throughput and may pause the lower-priority lane between files. It must not terminate a promotion, leave a child process uncollected, or report cancellation before the worker has stopped.

## Required evidence and acceptance

Implementation requires synthetic tests for unchanged quick scans avoiding content reads, repeated hash-request coalescing, disconnect/reconnect, bounded backoff, cancellation, per-volume serialization and measured byte counters. Installed acceptance must use disposable fast and deliberately slow external media under concurrent ingest/playback. Record drive type, connection, corpus, elapsed time, throughput, process count, CPU/RAM and worker states. No production card or Master Bin deletion is part of this acceptance.
