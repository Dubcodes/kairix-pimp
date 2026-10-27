# Data model

## Authoritative layout

```text
Master Bin/
  YYYY-MM-DD/...
  .pimp/
    events/<device-id>.jsonl
    migration-v2.json
    catalogue.json                       # legacy, retained read-only
    catalogue.pre-events-v1.backup.json # migration recovery copy
Working Project/
  .pimp-project.json
  existing Premiere, music, graphics, image and asset directories
```

Schema-2 files under `events/` are authoritative. Each device appends and flushes complete newline-delimited envelopes only to its own stream. The native catalogue is rebuilt from those streams; `catalogue.json` is never updated after its idempotent migration and is not authoritative. Syncthing conflict-copy event files are reported and quarantined from automatic replay.

Device settings, absolute project-path mappings, pending project-manifest reconciliation, discovery observations, ingest jobs, operation journals, local availability and the thumbnail cache are device-local. `ingest-jobs.json` is a recoverably replaced versioned array beside device settings. Each job records stable job/file/media IDs, source inventory, planned destinations, lifecycle and byte progress, failures, and provisional metadata. Provisional metadata includes an explicit edited-field mask so untouched defaults cannot erase an existing duplicate's metadata. Completed-file edits remain in a durable pending slot until their authoritative event append succeeds. Ingest staging is a hidden sibling of Master Bin on the same filesystem rather than a child of the synchronized tree. A future SQLite/FTS database remains a disposable local projection and must never be synchronized.

The remembered ingest date mode is a device preference, not shared production metadata. Legacy `shared-settings.json` is retained but no longer overwritten; this removes its former last-writer-wins role without discarding it.

## Identities and records

- Media ID: stable UUID assigned at registration; it is not content-derived. Full-file SHA-256 and normalized Master Bin relative path are independent identity evidence. During replay, registrations with the same path and hash resolve to the causally/deterministically earliest ID, with every other historic ID retained as a derived alias for event and project-reference reconciliation. Same hashes at different paths remain distinct; matching paths with different hashes remain distinct and produce an identity conflict. Migration-priority registrations preserve existing IDs where possible without rewriting event logs.
- Folder and project IDs: UUIDs, independent of names and paths.
- Paths: normalized relative paths rooted at Master Bin or a project.
- Date: ISO 8601 value plus `embedded`, `filesystem`, or `unknown` provenance; timezone retained when available.
- Source provenance: an optional portable source-relative path records the camera-card/package location used at ingest. It never stores a device-local source root in shared metadata.
- Manual fields include tags, project memberships, notes and favourite state. Incomplete ingest jobs retain these fields as provisional local intent; registration copies them into the media record, while later edits append normal authoritative events.
- Manual fields and AI results are separate. AI records include model ID/version, time range, generated time, and optional confidence.

## Events and conflicts

Every canonical envelope contains schema version `2`, a globally unique event ID, device ID, monotonically increasing per-device sequence, informational wall-clock timestamp, observed per-device sequence vector, and a typed `event` payload. Replay deduplicates event IDs and uses sequence/observed causality rather than clocks.

Tags and project assignments use add/remove events. Values are trimmed and grouped by Unicode lowercase key; a deterministic original spelling is retained so Rust and TypeScript projections agree regardless of delivery order. A removal affects only matching additions causally observed by the removing device, so an unrelated concurrent addition survives. Compatibility-only replacement events preserve v1 set/update semantics. Replace-style values such as notes and favourite state retain deterministic last-causal-write projection behavior, and concurrent scalar writes are retained in `conflicts`; the projection chooses a deterministic display value without destroying either event. File evidence is keyed by observing device; explicit SHA-256 verification changes only that device's warning.

Malformed lines, truncated streams, non-monotonic sequences, owner/filename mismatches and unsupported newer schemas are reported with source locations. Valid records from the same log remain usable. Original logs are never rewritten. Compatible v1 TypeScript and Rust envelopes are read through adapters; the final legacy snapshot is converted to deterministic bootstrap events, with the source snapshot and backup retained.

## Project manifest

`.pimp-project.json` is a portable generated projection containing project identity and media references (ID, Master Bin relative path, checksum, tags). Its working directory is `.`. Authoritative project identity, metadata and assignments live in events, so a delayed manifest cannot erase newer edits.

The writer recognizes a compatible existing manifest and imports its stable identity. Compatible updates preserve unknown fields. An incompatible, newer-schema or differently identified manifest is rejected without modification. Catalogue events remain authoritative if a later manifest write fails; a device-local durable retry record restores the portable projection after restart or temporary drive unavailability without emitting duplicate catalogue events. Updates use recoverable atomic replacement and never restructure or delete Premiere files, autosaves or asset directories.

Project inventory is ephemeral device-local evidence, not synchronized authority. It contains project ID, resolved local root, scan time, total file count and bytes, a relative main `.prproj` candidate, and categorized file entries with relative path, size and optional modification time. Refresh replaces only this in-memory view; it emits no catalogue event and changes no project asset.
