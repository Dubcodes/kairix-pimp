# Kairix PIMP development instructions

Kairix PIMP (Production Ingest & Media Processor) is a Windows-first desktop companion to Syncthing for safely ingesting, cataloguing, searching, assigning, and eventually archiving production media. Safety and accurate implementation claims take priority over speed.

## Read before changing code

Read this file, `README.md`, and the documents relevant to the task. At minimum consult:

- `docs/PROJECT_BRIEF.md` for product scope and current status.
- `docs/ARCHITECTURE.md` and `docs/DATA_MODEL.md` for boundaries and portable records.
- `docs/DOMAIN_RULES.md` for testable non-negotiable behavior.
- `docs/DECISIONS.md` before changing an established technical direction.
- `docs/TESTING.md` for the verification matrix and exact commands.
- `docs/SYNCTHING_DESIGN.md` for synchronization-sensitive changes.

Use `docs/AI_TASK_TEMPLATE.md` to frame substantial future work.

## Mandatory repository-local workspace

- Perform all development in `J:\projects\kairix-pimp`.
- **Never run `git worktree add`; never create an external Git worktree or another working copy.**
- Keep source, generated artifacts, screenshots, logs, fixtures, and configurable temporary files beneath this repository. Ignore ephemeral outputs appropriately.
- Do not modify other Kairix projects or create unrelated repositories.
- Before and after work, verify `git status`, the branch/remotes, and `git worktree list`.
- Do not force-push, rebase published history, or otherwise rewrite published history.

## Architecture boundaries

- Keep `packages/core` platform-neutral: no browser, Tauri, Node filesystem, OS, Premiere, or AI-runtime dependencies.
- Put filesystem/process integration behind adapters. The Node adapter is the current tested reference; Tauri Rust is the production host boundary.
- Give the React UI purpose-built application operations, not broad arbitrary filesystem access.
- Use stable media/project/folder IDs and portable relative paths. Absolute roots are device-local.
- Treat FFmpeg/FFprobe and future AI as bounded worker/process integrations with explicit arguments, cancellation, structured results, and retryable jobs.
- Use the shared `SearchableComboBox` for growing entity lists; do not create page-specific dropdown behavior. Keep ordinary searches, switches, actions, and short fixed choices as their native controls.

## File and media safety

- Treat original media as immutable. Never delete, truncate, rewrite, or rename a source file.
- Never silently overwrite an existing destination. Detect collisions and report ambiguity.
- Destination writes use a sibling temporary file, independent source/destination checksums, and atomic promotion.
- Reject paths outside configured roots after canonical resolution; string-prefix checks are insufficient.
- Make operations idempotent and interruption-safe where practical. Tests use synthetic media and repository-local or OS-provided disposable fixture directories only.
- Do not add source deletion or destructive cleanup without a separately reviewed design, explicit authorization, archive-independence verification, and recoverable quarantine.

## Syncthing and metadata

- Syncthing remains the synchronization engine; do not implement a competing sync service.
- Shared authoritative metadata must tolerate eventual synchronization, offline edits, duplicate delivery, and conflict copies.
- Each device appends only to its own event log. Never mutate another device's log.
- Never synchronize an open SQLite database as authoritative state. SQLite/FTS indexes are disposable local projections.
- Report malformed, unsupported, incomplete, or conflicting metadata; do not guess or silently discard it.

## Editions and external integrations

- Light contains the complete non-AI application and may read compatible AI results, but must not import or ship AI runtimes, models, or processing dependencies.
- Full uses the same catalogue and schemas and adds optional, isolated local AI workers. AI must not block ingest or browsing.
- Do not claim Premiere functionality without testing against a supported Premiere installation and documented UXP APIs. Never manipulate undocumented `.prproj` internals.
- Preserve the exact ingest labels **Use today's date** and **Use original file dates** as visible radio choices.

## Tests, documentation, and Git

- Add focused tests for changed behavior, including failure and safety paths. UI controls require keyboard and accessibility coverage.
- Run `npm test`, `npm run lint`, `npm run format`, `npm run build`, and `npm audit --audit-level=high` before committing. Run native checks only when prerequisites exist; never present browser verification as a native Tauri test.
- Keep README status claims, the roadmap, relevant architecture/product documents, domain rules, decisions, and testing instructions accurate in the same change.
- Do not add secrets, personal media, catalogues, Syncthing configuration, credentials, machine-specific paths, build output, or unlicensed third-party assets to this public repository.
- Inspect the complete diff before an ordinary commit and safe push. No force pushes.

## Final development report

Report what changed, files/docs affected, important rules preserved, exact checks executed and their results, screenshots where useful, remaining limitations, Git commit/push/CI status, and the recommended next milestone. Clearly distinguish implemented, tested, scaffolded, and planned behavior.
