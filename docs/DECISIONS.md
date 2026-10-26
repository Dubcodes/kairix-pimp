# Decision log

Known early decisions predate this log; their original dates were not recorded. They are recorded here on 2026-10-22 without inventing historical dates.

## ADR-001 — Tauri desktop shell

- **Recorded:** 2026-10-22 (original decision date not recorded)
- **Status:** Accepted
- **Context:** PIMP needs a desktop UI with restricted native filesystem/process access and future platform options.
- **Options:** Tauri 2; Electron; a browser-only application.
- **Decision:** Tauri 2 with React/TypeScript and a small Rust host.
- **Rationale:** Smaller runtime distribution and explicit capability/command boundaries while keeping a mature UI layer.
- **Consequences:** Rust/MSVC/WebView prerequisites are required for native Windows verification; browser builds do not verify the host.

## ADR-002 — Syncthing remains external

- **Recorded:** 2026-10-22 (original decision date not recorded)
- **Status:** Accepted
- **Context:** Files already move between devices with Syncthing.
- **Options:** Build synchronization; embed Syncthing; remain a companion.
- **Decision:** Use Syncthing as the only synchronization engine; optional REST access is read-only status information.
- **Rationale:** Avoids a second distributed system and preserves offline operation.
- **Consequences:** PIMP must tolerate eventual delivery, conflicts, temporary files, and different local roots.

## ADR-003 — Portable event metadata, local search projection

- **Recorded:** 2026-10-22 (original decision date not recorded)
- **Status:** Accepted
- **Context:** Synchronizing a live SQLite database is unsafe.
- **Options:** Shared SQLite; mutable per-item sidecars; per-device append-only events plus a derived projection.
- **Decision:** Synchronize portable append-only JSONL events partitioned by device; rebuild local SQLite/FTS projections.
- **Rationale:** Minimizes same-file write conflicts and makes duplicate delivery/replay tractable.
- **Consequences:** Projection, compaction, migration, and conflict UX are required; SQLite is disposable.

## ADR-004 — Shared Light/Full formats with isolated AI

- **Recorded:** 2026-10-22 (original decision date not recorded)
- **Status:** Accepted
- **Context:** A small non-AI installer and richer optional local analysis must interoperate.
- **Options:** Separate products/schemas; one build with all AI dependencies; shared app with optional worker.
- **Decision:** Light and Full share catalogue/project schemas; Full adds process-isolated optional workers.
- **Rationale:** Prevents format fragmentation and keeps ordinary workflows independent of AI.
- **Consequences:** Packaging must prove Light excludes models and inference runtimes.

## ADR-005 — Windows-first, portable boundaries

- **Recorded:** 2026-10-22 (original decision date not recorded)
- **Status:** Accepted
- **Context:** Premiere and the initial environment prioritize Windows, but future platforms are required.
- **Options:** Windows-only implementation; lowest-common-denominator implementation; Windows-first adapters with portable core.
- **Decision:** Validate Windows first while keeping core records/rules platform-neutral and OS behavior behind adapters.
- **Rationale:** Delivers the primary workflow without embedding Windows paths or APIs in domain state.
- **Consequences:** Linux/macOS remain unverified until later platform-specific testing.

## ADR-006 — Verified ingest and immutable originals

- **Recorded:** 2026-10-22 (original decision date not recorded)
- **Status:** Accepted
- **Context:** Production footage loss is unacceptable.
- **Options:** Fast unchecked copy; post-copy spot checking; temporary copy with independent checksums and atomic promotion.
- **Decision:** Preserve sources; copy to a sibling temporary file, independently hash source/destination, then atomically promote without overwrite.
- **Rationale:** Reliability is more important than peak throughput.
- **Consequences:** Ingest takes extra I/O and must expose progress/cancellation.

## ADR-007 — Date organization modes and provenance

- **Recorded:** 2026-10-22 (original decision date not recorded)
- **Status:** Accepted
- **Context:** A batch may need one ingest date or per-file capture dates, which may be absent or wrong.
- **Options:** One implicit policy; editable arbitrary folder; two explicit remembered modes.
- **Decision:** Expose exactly **Use today's date** and **Use original file dates**, while retaining ingest and original-date provenance/timezone.
- **Rationale:** Keeps the common choice clear without losing evidence.
- **Consequences:** Invalid/ambiguous dates require visible fallback handling.

## ADR-008 — Project-local manifests and many-to-many assignments

- **Recorded:** 2026-10-22 (original decision date not recorded)
- **Status:** Accepted
- **Context:** Clips may serve multiple edits whose absolute working paths differ by device.
- **Options:** Single project field; central-only assignment; stable project IDs plus project-local manifest.
- **Decision:** Store `projectIds[]` on projected media and an atomic `.pimp-project.json` in each working project.
- **Rationale:** Supports portable many-to-many relationships without disturbing existing assets.
- **Consequences:** Cleanup must aggregate references across every active project.

## ADR-009 — Common searchable combo-box control

- **Date:** 2026-10-22
- **Status:** Accepted
- **Context:** Browser prompts and page-specific dropdowns cannot provide accessible, searchable, stable-ID selection at scale.
- **Options:** Custom behavior per page; a large visual framework; Downshift; React Aria; Ariakit headless combobox.
- **Decision:** Build one Kairix-styled `SearchableComboBox` on Ariakit, supporting single/multiple selection and explicit creation.
- **Rationale:** Ariakit provides maintained WAI-ARIA behavior, keyboard/focus handling, and portal/edge-aware positioning without imposing a UI framework. Kairix retains filtering and data semantics.
- **Consequences:** `@ariakit/react` is a production dependency; UI tests cover the shared contract. Rendering is capped at 100 filtered options, so users narrow very large lists by typing; true windowed virtualization can be added if measured data sizes require it.

## ADR-010 — Direct Master Bin editing workflow

- **Date:** 2026-10-22
- **Status:** Accepted
- **Decision:** Projects reference footage directly from Master Bin by default. PIMP leaves existing project directories intact and relies on Premiere's normal user-managed drive-path relinking between computers until a tested UXP extension exists.

## ADR-011 — Temporary ingest-date overrides

- **Date:** 2026-10-22
- **Status:** Accepted
- **Decision:** Today, Yesterday and Choose date are request-scoped overrides. They do not become a third persistent organization mode or modify the remembered two-mode preference.

## ADR-012 — Configurable catalogue with migration lock

- **Date:** 2026-10-22
- **Status:** Accepted
- **Decision:** Shared metadata may live under `Master Bin/.pimp` or at an explicit shared path. Once initialized, the native host rejects location changes until a verified migration workflow exists rather than silently abandoning data.

## ADR-013 — Discovery and future location-aware records

- **Date:** 2026-10-22
- **Status:** Accepted
- **Decision:** The initial host performs stable-file reconciliation periodically and on demand without moving external copies. A notification watcher, multiple physical locations, independent availability/verification state, offline Cathy records, system theme and optional grid/list layout remain compatible future extensions rather than claims of current implementation.

## ADR-014 — Schema-2 causal event authority

- **Date:** 2026-10-22
- **Status:** Accepted
- **Decision:** Per-device schema-2 JSONL streams are the sole authoritative catalogue metadata. Envelopes carry event/device IDs, monotonic device sequence, observed sequence vector and typed payload. Tags and project memberships use causal add/remove events; concurrent scalar writes are retained as conflicts. Wall-clock time is informational only.
- **Consequences:** The native projection is rebuilt from events. `catalogue.json` becomes a retained legacy input after idempotent migration. Conflict resolution UI and eventual compaction remain separate work.

## ADR-015 — Migration without destructive conversion

- **Date:** 2026-10-22
- **Status:** Accepted
- **Decision:** Existing snapshots become deterministic bootstrap events while their original bytes and a recovery backup remain intact. V1 TypeScript and Rust envelopes are adapted during replay; newer unsupported records are reported and untouched.
- **Consequences:** Migration can be restarted safely and preserves IDs and final snapshot metadata. Damaged snapshots stop migration instead of creating an empty library.

## ADR-016 — Private same-filesystem ingest staging

- **Date:** 2026-10-22
- **Status:** Accepted
- **Decision:** Native ingest stages in a device-owned hidden sibling of Master Bin, never inside the synchronized tree. A small local journal identifies owned incomplete work. Verified files are promoted using an OS no-replace operation.
- **Consequences:** A Master Bin whose parent cannot host private staging fails closed. Journals recover abandoned staging but do not yet resume byte ranges.

## ADR-017 — Conservative evidence-based discovery

- **Date:** 2026-10-22
- **Status:** Accepted
- **Decision:** External files require repeated unchanged observations spanning at least 15 seconds before hashing and registration. Existing paths are not permanently skipped; changed evidence triggers revalidation and review.
- **Consequences:** Discovery intentionally has latency and still cannot prove an external producer has closed a file. A watcher or Syncthing completion signal may strengthen this later.

## ADR-018 — Portable thumbnail references and generated manifests

- **Date:** 2026-10-22
- **Status:** Accepted
- **Decision:** Catalogue thumbnail values are cache-relative filenames resolved beneath the configured local cache. Project manifests are portable generated projections; incompatible existing identities are rejected rather than overwritten.
- **Consequences:** Absolute legacy thumbnail paths are reduced to safe cache references. Cache-budget eviction and field-level project conflict UX remain deferred.

## ADR-019 — DCM naming and configurable date presentation

- **Date:** 2026-10-23
- **Status:** Accepted
- **Decision:** Use **DCM Master Bin** as the product term and default root-folder name. Retain the two organization modes, use `yyyymmdd` (an alias of `yyyyMMdd`) for new installations, validate date/time tokens as a filename, and never rename existing date folders when that preference changes. Existing settings without the preference retain `yyyy-MM-dd`.
- **Consequences:** Date formatting is presentation/configuration work, not a migration of existing media paths.

## ADR-020 — Durable ingest intent metadata

- **Date:** 2026-10-23
- **Status:** Accepted
- **Decision:** Ingest jobs may carry tags, project assignments, descriptions and favourites. Queued/incomplete intent is persisted independently and survives retry; it becomes catalogue metadata only after verified promotion. Do not impose a rigid tag-count cap.
- **Consequences:** The persistent job schema needs versioned intent fields and must not expose an incomplete item as committed media.

## ADR-021 — Direct Premiere layout and explicit project collection

- **Date:** 2026-10-23
- **Status:** Accepted
- **Decision:** Premiere edits directly from DCM Master Bin, with dated bins and an optional virtual Audio grouping. A collected `.prproj` inside DCM Content becomes the authoritative editing project for the existing stable project ID only after an explicit verified collection workflow; file presence alone does not imply collection.
- **Consequences:** Collection needs a state event, identity validation, dependency verification and rollback-safe archive construction.

## ADR-022 — Verification-gated, recoverable cleanup

- **Date:** 2026-10-23
- **Status:** Accepted
- **Decision:** Cleanup automation is future, opt-in and disabled by default. Eligibility requires successful finalization verification, per-file protection and cross-project reference checks. Removal first uses recoverable quarantine; Syncthing replicas do not count as backups.
- **Consequences:** No deletion behavior is added in the reliability milestone. Later purge requires separate authorization and retention policy.

## ADR-023 — Searchable completed-project records and persistent jobs

- **Date:** 2026-10-23
- **Status:** Accepted
- **Decision:** Keep completed projects as lightweight searchable catalogue records and give each portable local archive its stable project identity and media manifest. Public-link and YouTube metadata remain deferred. Long-running work uses a persistent job-manager pattern, implemented first for ingest.
- **Consequences:** Project history remains usable when archive media is offline; job persistence must distinguish intent, progress, committed effects and retryable enrichment.

## ADR-024 — Device-local persistent ingest queue

- **Date:** 2026-10-24
- **Status:** Accepted
- **Decision:** Native ingest is coordinated by a versioned device-local job store and a bounded single worker. Jobs persist stable source/destination plans, progress, errors and provisional metadata. Copying, hashing and probing occur outside the catalogue write mutex; only verified registration and authoritative event persistence use it. Pause is supported between files. Startup marks active work interrupted and requires explicit retry rather than assuming removable media is present.
- **Consequences:** Completed files form safe retry checkpoints, but partial-file byte offsets are not resumed. Operation journals remain the ownership and crash-recovery record for staging and promotion. Queue state and file availability are not synchronized through Syncthing.

## ADR-025 — Unsigned beta packaging and constrained Windows file actions

- **Date:** 2026-10-25
- **Status:** Accepted
- **Decision:** CI builds unsigned MSI and NSIS artifacts identified by version and commit. Versioned beta tags may publish prereleases only after review and merge. FFmpeg/FFprobe remain optional PATH-based tools for this beta. Reveal/open/copy UI actions accept only catalogue media IDs and resolve an existing path canonically beneath Master Bin in the native host.
- **Consequences:** SmartScreen warnings and optional-media-tool limitations are explicit. The UI receives no arbitrary shell or filesystem capability. Signing, installer upgrades and bundled third-party media binaries remain later release work.

## ADR-026 — Recovery-safe job retention and package-coherent planning

- **Date:** 2026-10-26
- **Status:** Accepted
- **Decision:** Automatically bound only fully completed ingest jobs with no pending metadata or recovery evidence. Failed, cancelled, interrupted and otherwise recoverable jobs remain until a future explicit user-authorized discard operation exists. When separately selected roots would collide, retain deterministic source-root identity in the planned relative path. In original-file mode, detected camera packages use their earliest available member modification time as one package date; independent files continue to use their own times.
- **Consequences:** Repeated failures can grow the local job store until the user has an explicit discard control, but retry intent and evidence are never silently lost. Package sidecars stay with their media, and pre-copy path previews match queued path planning.

## ADR-027 — Device-local desktop shell preferences

- **Date:** 2026-10-26
- **Status:** Accepted
- **Decision:** Close-to-tray defaults on for new devices, but the close event is intercepted only after native tray creation succeeds. Show restores the existing single instance; Quit exits without deleting durable job state. Appearance, density, startup page, update-check intent and launch-at-login remain device-local settings. External support links use a fixed native destination allowlist. The supported beta installer is per-user NSIS; MSI remains a compatibility artifact and another installation scope is never silently removed.
- **Why:** Background ingest/discovery must survive an ordinary window close without making PIMP inaccessible, synced metadata must not acquire machine preferences, web content must not gain arbitrary URL-opening authority, and upgrade cleanup must preserve user data and explicit control.
- **Consequences:** Tray behavior still requires installed-Windows acceptance, and users with both early MSI and NSIS installations follow the documented verify-then-uninstall migration.

## ADR-028 — Fail-closed signed Beta update channel

- **Date:** 2026-10-26
- **Status:** Accepted
- **Decision:** In-app updates use only the official Tauri 2 updater, an embedded minisign public key and the fixed HTTPS `beta-channel/latest.json` release asset. Only reviewed beta tags with protected signing secrets may publish that manifest and signed NSIS updater payload. Beta builds use a numeric SemVer prerelease component (for example `0.1.1-1`) so the same version remains valid for Windows MSI packaging; the release channel remains explicitly Beta. The application blocks installation during active ingest. Missing key configuration is an actionable unavailable state, never “up to date.”
- **Consequences:** The source and CI foundation are complete, but in-app installation remains provisionally disabled until the release owner provisions the protected private key/password and matching public repository variable. Manual per-user NSIS installation remains supported.

## ADR-029 — Serialized device settings and tray-gated autostart

- **Date:** 2026-10-26
- **Status:** Accepted
- **Decision:** The desktop serializes every accepted immediate preference write and applies a native response only when it belongs to the newest local revision. Failed latest writes remain visibly retryable, pending writes warn before ordinary WebView unload, and explicit interface reload waits for the queue. Windows `--minimized` startup hides the main window only when native tray construction succeeded.
- **Consequences:** Rapid theme, density, tray, startup-page and update-check changes remain ordered without stale UI rollback. Configuration form values still use explicit save to avoid excessive writes. If tray creation fails during autostart, PIMP remains visible and logs the fallback instead of becoming inaccessible.
