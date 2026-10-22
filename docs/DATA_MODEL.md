# Data model

## Authoritative layout

```text
Master Bin/
  YYYY-MM-DD/...
  .pimp/
    catalogue.json
    shared-settings.json
    events/<device-id>.jsonl
Working Project/
  .pimp-project.json
  existing Premiere, music, graphics, image and asset directories
```

`catalogue.json` is the current native read model. The host also appends audit records only to its device-partitioned JSON Lines file. Cross-device replay of those files into the snapshot is not implemented yet, so this milestone must not be represented as fully Syncthing-conflict-safe. Device settings, absolute project-path mappings and the bounded thumbnail cache live in the operating-system application-data location (or another explicitly configured device-local cache), not in portable metadata. A future local SQLite database remains a disposable projection and must never be synchronized.

## Identities and records

- Media ID: UUID for the catalogue item; full-file SHA-256 is stored separately for integrity and duplicate detection. This permits future location records and intentional identical copies.
- Folder and project IDs: UUIDs, independent of names and paths.
- Paths: normalized relative paths rooted at Master Bin or a project.
- Date: ISO 8601 value plus `embedded`, `filesystem`, or `unknown` provenance; timezone retained when available.
- Manual fields and AI results are separate. AI records include model ID/version, time range, generated time, and optional confidence.

## Events and conflicts

Event envelopes contain schema version, globally unique event ID, device ID, per-device sequence, timestamp, and payload. Replay deduplicates event IDs. Deterministic ordering makes projection repeatable, but equal-time edits from different devices to the same replace-style field are retained as conflicts for user resolution.

Future schema migrations are pure, version-to-version transforms. Unknown newer schema versions are quarantined from projection and reported. Malformed lines are reported with filename and line rather than discarded silently.

## Project manifest

`.pimp-project.json` embeds project identity and portable media references (ID, Master Bin relative path, checksum, tags). Its working directory is `.`. Updating the manifest uses atomic replacement and does not recreate or delete asset directories.

The manifest writer creates the stable project record without restructuring or deleting existing project content. Additive media assignments update both the catalogue and each affected manifest with portable media references; absolute project paths remain device-local.
