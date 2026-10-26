# Kairix PIMP

> **Unsigned internal beta:** Windows installer artifacts are produced for
> disposable-media testing. PIMP is not yet approved for production media while
> real two-device Syncthing acceptance, signing and upgrade testing remain open.

Kairix PIMP (Production Ingest & Media Processor) is a desktop companion for Syncthing that safely ingests, catalogues, searches, and assigns production media. Syncthing remains the synchronization engine; PIMP owns media workflow and portable metadata.

PIMP does not replace Syncthing or implement another synchronization service. A synchronized **Master Bin** holds media and authoritative portable metadata, while **Working Projects** hold active Premiere projects and their assets. Absolute paths stay device-local.

## Editions

- **PIMP Light** is the complete non-AI application: verified ingest, catalogue and search, metadata, project assignment, Premiere integration, and archiving workflows. It can read AI metadata generated elsewhere but will not ship AI models or inference runtimes.
- **PIMP Full** will use the same catalogue and application, with optional local workers for transcription, timestamped search, scene and activity descriptions, sound recognition, and suggested tags.

Full-edition AI processing is planned and is not implemented in this milestone.

## Milestone status

The Windows vertical slice now connects the React interface to a restricted Tauri 2 host. Versioned per-device append-only event streams are authoritative; the Library is rebuilt by deterministic topological replay and no synchronized mutable database is operational state. The host accepts native file/folder pickers and drops, inventories every discovered file before copying, and queues recursive ingest as durable device-local background jobs. Common production video, audio and still formats plus camera sidecars and unknown package files are retained; deliberate temporary-file exclusions and unreadable/traversal failures are reported. Camera-package and duplicate-name folder structure is preserved with source-relative provenance, including distinct roots selected in one ingest, and planned relative destinations are reviewable before copying. A bounded single worker independently verifies staged copies and promotes them without overwrite while the Library remains usable. The compact job view reports overall and per-file progress, inventory omissions, failures and verification state, and provides between-file pause, resume, cancellation and retry controls. A single-instance guard prevents two desktop processes from writing the same device stream.

The Library displays genuine persisted records, device-local availability, technical metadata and FFmpeg thumbnails. Search, desktop-style click/Ctrl/Shift/Ctrl+A selection, batch add/remove tags and projects, batch descriptions and favourites, many-to-many project assignment, folder-aware MP4/WAV pairing, quick/full scan controls and conservative multi-observation reconciliation survive restarts. Configured libraries begin a background scan at startup, repeat after the stability interval when candidates are pending, and continue periodic discovery without blocking navigation. Scan progress and the last result are device-local. Multi-path clipboard copy and Explorer actions provide an explicit manual Premiere handoff; no Premiere API automation is claimed. Lightweight polling refreshes shared event metadata and separately notices when an already-catalogued path arrives through Syncthing, without publishing availability as shared truth. Concurrent scalar edits are retained as visible conflicts. New installs show setup guidance and an empty library instead of synthetic media.

Legacy `catalogue.json`, v1 TypeScript events, v1 Rust events and compatible project manifests have explicit compatibility paths. Migration preserves the original snapshot plus a recoverable backup and is idempotent. Ingest uses a device-owned same-filesystem staging directory outside Master Bin, durable operation journals plus a versioned local job store, source-change detection and atomic no-overwrite promotion. Tags, project assignments, notes and favourite intent may be edited while copying and are committed only after successful media registration.

New installations use `yyyymmdd` date folders. Settings provide a live preview based on the current local date/time and support `yyyy`, `MM`, `dd`, `HH`, `hh`, `mm`, `ss`, and `a`; older settings retain their existing dashed pattern until explicitly changed. Today and explicit-date batches use one batch timestamp. Original-file mode uses each independent file's modified date, while every file in a detected camera package uses the earliest available package-member modification time so its hierarchy is never split across date folders.

Settings are organized into General, Library & Storage, Ingest & Discovery, Appearance, Updates, About & Support and Diagnostics. Graphite is the default token-based theme, with Adobe-style Dark, Light, Midnight, System and High Contrast alternatives. Close-to-tray defaults on for new devices; the native notification-area menu restores the existing single instance or explicitly quits. Safe support links are selected from a native allowlist. The recommended Windows beta package is the per-user NSIS installer; see [Windows installation and upgrades](docs/WINDOWS_INSTALLATION.md).

Configure DCM Master Bin itself as the Syncthing folder. If a broader parent directory is synchronized, explicitly exclude the sibling `.DCM Master Bin.pimp-staging-*` path so incomplete device-local staging is never replicated.

The native host, web frontend, unsigned MSI and NSIS installers build on Windows. Installer signing and upgrades, a notification-based filesystem watcher, safe catalogue-location relocation, folder notes, preview-frame selection and automated Premiere integration remain incomplete.

Successful **Windows beta installer** workflow runs publish a downloadable MSI
and NSIS installer artifact identified by version, commit and unsigned status,
with SHA-256 checksums for both installers.
See the [Windows beta installation and acceptance guide](docs/BETA_TESTING.md).
FFmpeg/FFprobe are optional PATH-based tools: missing tools disable thumbnails
and technical probing but do not block verified ingest or catalogue work.

Windows is the first release target. The core and adapter boundaries are designed for future Linux and macOS support, but neither platform is currently verified.

## Development

Prerequisites for the verified web/core workflow:

- Node.js 22+ and npm 10+
- FFmpeg/FFprobe on `PATH` for media probing and thumbnails

Install and run:

```powershell
npm install
npm run dev
```

Open <http://localhost:1420>. Quality checks:

```powershell
npm test
npm run lint
npm run format
npm run build
```

For the native application, additionally install the current stable Rust toolchain, Microsoft C++ Build Tools with the Desktop C++ workload, and WebView2. Then run:

```powershell
npm run tauri dev
```

## Planned work

- Byte-range ingest resume, throughput history and a notification-based watcher
- Virtualized browsing, saved compound filters, contact sheets, and previews
- A tested Adobe Premiere UXP extension and import reconciliation
- Optional, offline local media intelligence in PIMP Full
- Verified complete-source and consolidated project archives
- Synchronization-aware, recoverable cleanup with explicit authorization
- Windows Light and Full installers, followed by Linux and macOS validation

See the [project brief](docs/PROJECT_BRIEF.md), [development roadmap](docs/DEVELOPMENT_ROADMAP.md), and [testing strategy](docs/TESTING.md) for scope and evidence boundaries.

## Known limitations

- Event replay and convergence are simulated in automated tests, but actual multi-device Syncthing conflict behavior still needs an external two-device acceptance pass.
- Automatic discovery starts at launch, follows pending candidates after the 15-second stability interval, and polls every 30 seconds. A native watcher and positive transfer-completion signal are not yet wired.
- Catalogue relocation is deliberately locked after initialization until safe migration is implemented.
- Interrupted jobs retain intent and verified completed files, but an incomplete current file restarts from byte zero when explicitly retried; jobs are not automatically resumed when removable source media may be absent.
- Metadata conflicts are reported as a count; field-level conflict resolution UI remains planned.
- Thumbnail cache references are portable and root-validated, but configured-budget eviction remains deferred.
- Manual multi-path Premiere handoff is available; automated Premiere integration, AI processing, project consolidation, and cleanup are not functional.
- Unsigned MSI and NSIS bundles build and the packaged native workflow is verified; release signing and upgrade behavior remain unverified.

## License

No software license has been selected yet. Public availability does not grant permission to use, modify, or redistribute the code. A license decision remains with the project owner.

## Repository map

- `apps/desktop`: React/TypeScript UI
- `packages/core`: platform-neutral catalogue, dates, search, and AI contracts
- `packages/node-adapter`: verified filesystem operations and FFmpeg integration
- `src-tauri`: restricted Tauri 2 filesystem/catalogue host and dialog capability
- `docs`: project brief, product/architecture/data rules, decision log, testing strategy, Syncthing design, research, roadmap, and future task template

The future cloud-backup boundary is documented in the
[provider-neutral backup manifest](docs/BACKUP_MANIFEST.md). No current feature
depends on Degoo or another cloud provider.

## Development preview

![Kairix PIMP native Library after verified MP4/WAV ingest and restart](docs/pimp-native-library.png)
