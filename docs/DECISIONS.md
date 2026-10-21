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
