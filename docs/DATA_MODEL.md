# Data model

## Authoritative layout

```text
Master Bin/
  media/YYYY-MM-DD/...
  .pimp/
    catalogue.json
    settings.json
    events/<device-id>.jsonl
    previews/<media-id>/...
    ai/<media-id>/<processor-version>.json
Working Project/
  .pimp-project.json
  existing Premiere, music, graphics, image and asset directories
```

`catalogue.json` identifies the format; state is derived from immutable JSON Lines events. A device appends only to its own file. A local SQLite database is a disposable projection and is never synchronized.

## Identities and records

- Media ID: currently `sha256:<full-file-sha256>`. A later alias/resource layer can distinguish two intentional identical copies without changing content identity.
- Folder and project IDs: UUIDs, independent of names and paths.
- Paths: normalized relative paths rooted at Master Bin or a project.
- Date: ISO 8601 value plus `embedded`, `filesystem`, or `unknown` provenance; timezone retained when available.
- Manual fields and AI results are separate. AI records include model ID/version, time range, generated time, and optional confidence.

## Events and conflicts

Event envelopes contain schema version, globally unique event ID, device ID, per-device sequence, timestamp, and payload. Replay deduplicates event IDs. Deterministic ordering makes projection repeatable, but equal-time edits from different devices to the same replace-style field are retained as conflicts for user resolution.

Future schema migrations are pure, version-to-version transforms. Unknown newer schema versions are quarantined from projection and reported. Malformed lines are reported with filename and line rather than discarded silently.

## Project manifest

`.pimp-project.json` embeds project identity and portable media references (ID, Master Bin relative path, checksum, tags). Its working directory is `.`. Updating the manifest uses atomic replacement and does not recreate or delete asset directories.
