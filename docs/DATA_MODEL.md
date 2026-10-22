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

Device settings, absolute project-path mappings, discovery observations, ingest journals and the thumbnail cache are device-local. Ingest staging is a hidden sibling of Master Bin on the same filesystem rather than a child of the synchronized tree. A future SQLite/FTS database remains a disposable local projection and must never be synchronized.

The remembered ingest date mode is a device preference, not shared production metadata. Legacy `shared-settings.json` is retained but no longer overwritten; this removes its former last-writer-wins role without discarding it.

## Identities and records

- Media ID: stable UUID for the catalogue item; full-file SHA-256 is stored separately for integrity and duplicate detection. Migration preserves existing IDs.
- Folder and project IDs: UUIDs, independent of names and paths.
- Paths: normalized relative paths rooted at Master Bin or a project.
- Date: ISO 8601 value plus `embedded`, `filesystem`, or `unknown` provenance; timezone retained when available.
- Manual fields and AI results are separate. AI records include model ID/version, time range, generated time, and optional confidence.

## Events and conflicts

Every canonical envelope contains schema version `2`, a globally unique event ID, device ID, monotonically increasing per-device sequence, informational wall-clock timestamp, observed per-device sequence vector, and a typed `event` payload. Replay deduplicates event IDs and uses sequence/observed causality rather than clocks.

Tags and project assignments use add/remove events. A removal affects only additions causally observed by the removing device, so an unrelated concurrent addition survives. Replace-style values such as notes retain concurrent values in `conflicts`; the projection chooses a deterministic display value without destroying either event.

Malformed lines, truncated streams, non-monotonic sequences, owner/filename mismatches and unsupported newer schemas are reported with source locations. Valid records from the same log remain usable. Original logs are never rewritten. Compatible v1 TypeScript and Rust envelopes are read through adapters; the final legacy snapshot is converted to deterministic bootstrap events, with the source snapshot and backup retained.

## Project manifest

`.pimp-project.json` is a portable generated projection containing project identity and media references (ID, Master Bin relative path, checksum, tags). Its working directory is `.`. Authoritative project identity, metadata and assignments live in events, so a delayed manifest cannot erase newer edits.

The writer recognizes a compatible existing manifest and imports its stable identity. An incompatible or differently identified manifest is rejected without modification. Updates use recoverable atomic replacement and never restructure or delete Premiere files, autosaves or asset directories.
