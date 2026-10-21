# Product specification

## Product promise

PIMP gives editors a trustworthy path from removable camera media to an organized, searchable Master Bin and portable editing projects. It complements Syncthing and never silently alters originals.

## Editions

**Light** includes ingest, verification, browsing, metadata, search, projects, Premiere integration, archive workflows, and viewing existing AI metadata. **Full** uses the same binaries and schemas plus optional, independently installable local AI workers and models. Core code must never import an AI runtime.

## Core workflows

1. Configure device-local Master Bin and Working Projects roots.
2. Ingest files/folders with exactly **Use today's date** or **Use original file dates**.
3. Copy to a temporary destination, independently hash source and destination, then atomically promote.
4. Catalogue using a content-derived stable media ID. Preserve real ingest time, capture date, provenance, and timezone when known.
5. Browse folders/media; tag folders and multi-selected media; assign any clip to multiple projects.
6. Generate a project-local portable manifest without disturbing existing project assets.
7. Rebuild the local search view from synchronized authoritative metadata.

## Safety invariants

- Original media is read-only to PIMP.
- Existing destination files are never overwritten silently.
- Cancellation removes only PIMP temporary files.
- Paths must remain inside configured roots after canonicalization.
- Conflicting edits are shown, not silently discarded.
- No initial-milestone source deletion or automated cleanup exists.

## Status vocabulary

- **Implemented:** code exists in the portable core/Node adapter.
- **Tested:** exercised by automated tests in this repository.
- **Scaffolded:** integration boundary exists but its host dependency is unavailable here.
- **Planned:** design only; no functional claim.

The exact current status is maintained in `DEVELOPMENT_ROADMAP.md`.
