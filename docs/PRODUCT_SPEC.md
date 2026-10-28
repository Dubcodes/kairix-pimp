# Product specification

## Product promise

PIMP gives editors a trustworthy path from removable camera media to an organized, searchable Master Bin and portable editing projects. It complements Syncthing and never silently alters originals.

## Editions

**Light** includes ingest, verification, browsing, metadata, search, projects, Premiere integration, archive workflows, and viewing existing AI metadata. **Full** uses the same binaries and schemas plus optional, independently installable local AI workers and models. Core code must never import an AI runtime.

## Core workflows

1. Configure device-local Master Bin and Working Projects roots.
2. Ingest files/folders with exactly **Use today's date** or **Use original file dates**.
3. Copy to a temporary destination, independently hash source and destination, then atomically promote.
4. Catalogue using a stable registration ID while storing relative path and SHA-256 as separate identity evidence. Replay deterministically reconciles independent registrations only when both normalized synchronized path and hash agree; it retains aliases for historic references without rewriting logs. Preserve real ingest time, capture date, provenance, and timezone when known.
5. Browse folders/media; tag folders and multi-selected media; assign any clip to multiple projects through stable-ID searchable selectors.
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

Non-negotiable observable behavior is maintained in `DOMAIN_RULES.md`; implemented versus external verification is maintained in `TESTING.md`.

## Confirmed product direction

These choices constrain future milestones; they are not claims that the current build implements them.

### Master Bin and ingest metadata

- The product term and default root-folder name is **DCM Master Bin**.
- Date organization keeps the existing two modes and never renames already-ingested folders. A future naming-pattern setting will support separators and 12-/24-hour time; its default will be `yyyyMMdd`.
- The ingest screen will support initial tags, project assignments, descriptions and favourites. Queued and incomplete jobs retain that metadata across retry, separate from committed catalogue records.
- Tags have no arbitrary fixed count limit; performance may use search, paging and measured rendering limits.

### Library and Premiere workflow

- The future library interaction model includes Ctrl-click and Shift-click selection, Ctrl+A for the filtered result, and right-click actions for **Go to this bin**, **Reveal in Explorer**, **Copy file path**, favourite/unfavourite, editing tags/descriptions, protection from automatic cleanup, and navigation between original and archived locations.
- Premiere references source media directly from DCM Master Bin. Dated folders map naturally to bins; an optional virtual **Audio** view may group paired audio without moving source files.
- When a project is completed, the collected `.prproj` under **DCM Content** becomes that stable project ID's authoritative editing project. Collection is an explicit verified state transition and is never inferred merely because a `.prproj` file exists.

### Finalization, cleanup and project history

- Finalization must verify copied/derived assets, project resolvability and archive independence before any source becomes cleanup-eligible.
- Future automatic cleanup is opt-in and disabled by default. It requires per-file protection, cross-project reference checks and a recoverable quarantine. Syncthing replicas are synchronization copies, not backups. This milestone adds no deletion behavior.
- Completed projects remain lightweight searchable catalogue records. A portable project-local archive carries the stable project ID, detailed metadata/transcripts/previews and media manifest. Exported videos may hold multiple public links. Future YouTube integration retains original title/description metadata and may create a search-optimized description that removes recognized recurring boilerplate without discarding music credits or project-specific text; both integrations are deferred.
- Long-running ingest, enrichment, consolidation and cleanup will move to a persistent background job manager that resumes at startup; the current minimal ingest journal is only the reliability foundation for that work.
