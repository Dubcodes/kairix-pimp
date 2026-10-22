# Kairix PIMP

> **Early development:** PIMP is not yet ready for production media, and no
> installable release is available.

Kairix PIMP (Production Ingest & Media Processor) is a desktop companion for Syncthing that safely ingests, catalogues, searches, and assigns production media. Syncthing remains the synchronization engine; PIMP owns media workflow and portable metadata.

PIMP does not replace Syncthing or implement another synchronization service. A synchronized **Master Bin** holds media and authoritative portable metadata, while **Working Projects** hold active Premiere projects and their assets. Absolute paths stay device-local.

## Editions

- **PIMP Light** is the complete non-AI application: verified ingest, catalogue and search, metadata, project assignment, Premiere integration, and archiving workflows. It can read AI metadata generated elsewhere but will not ship AI models or inference runtimes.
- **PIMP Full** will use the same catalogue and application, with optional local workers for transcription, timestamped search, scene and activity descriptions, sound recognition, and suggested tags.

Full-edition AI processing is planned and is not implemented in this milestone.

## Milestone status

The Windows vertical slice now connects the React interface to a restricted Tauri 2 host. It persists device settings, shared settings, a catalogue snapshot and per-device audit events; accepts native file/folder pickers and drops; recursively ingests supported camera media; independently SHA-256 verifies temporary copies before atomic promotion; and reports progress, duplicates, unsupported inputs and actionable filename conflicts.

The Library displays genuine persisted records, folders, availability, technical metadata and FFmpeg thumbnails. Search, additive bulk tags, many-to-many project assignment, MP4/WAV pairing, manual rescan and a 30-second stable-file reconciliation fallback survive restarts. New installs show setup guidance and an empty library instead of synthetic media. Fixtures remain available only to tests/reference code.

The native host, web frontend, unsigned MSI and NSIS installers build on Windows, and the packaged application has completed a native ingest/restart acceptance pass. Installer signing and upgrades, a durable restartable ingest journal, notification-based filesystem watcher, safe catalogue-location migration, folder notes, preview-frame selection and Premiere integration remain incomplete.

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

- Durable restartable ingest jobs and a notification-based watcher
- Virtualized browsing, saved compound filters, contact sheets, and previews
- A tested Adobe Premiere UXP extension and import reconciliation
- Optional, offline local media intelligence in PIMP Full
- Verified complete-source and consolidated project archives
- Synchronization-aware, recoverable cleanup with explicit authorization
- Windows Light and Full installers, followed by Linux and macOS validation

See the [project brief](docs/PROJECT_BRIEF.md), [development roadmap](docs/DEVELOPMENT_ROADMAP.md), and [testing strategy](docs/TESTING.md) for scope and evidence boundaries.

## Known limitations

- Cross-device event replay into the native catalogue snapshot is not yet wired.
- Automatic discovery currently polls every 30 seconds and offers manual Rescan; a native watcher is not yet wired.
- Catalogue relocation is deliberately locked after initialization until safe migration is implemented.
- No Premiere integration, AI processing, project consolidation, or cleanup is functional yet.
- Unsigned MSI and NSIS bundles build and the packaged native workflow is verified; release signing and upgrade behavior remain unverified.

## License

No software license has been selected yet. Public availability does not grant permission to use, modify, or redistribute the code. A license decision remains with the project owner.

## Repository map

- `apps/desktop`: React/TypeScript UI
- `packages/core`: platform-neutral catalogue, dates, search, and AI contracts
- `packages/node-adapter`: verified filesystem operations and FFmpeg integration
- `src-tauri`: restricted Tauri 2 filesystem/catalogue host and dialog capability
- `docs`: project brief, product/architecture/data rules, decision log, testing strategy, Syncthing design, research, roadmap, and future task template

## Development preview

![Kairix PIMP native Library after verified MP4/WAV ingest and restart](docs/pimp-native-library.png)
