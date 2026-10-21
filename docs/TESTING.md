# Testing and verification

Never mark a check passed unless it was executed against the reported revision. Browser UI tests do not verify native Tauri behavior.

## Current repository commands

From `J:\projects\kairix-pimp`:

```powershell
npm install
npm test
npm run lint
npm run format
npm run build
npm audit --audit-level=high
```

`npm test` runs portable core, Node adapter, and jsdom React component/integration tests. `npm run build` type-checks project references and creates the production web bundle. CI repeats these checks on Ubuntu with Node 24 and FFmpeg. Native Windows validation additionally requires Rust, MSVC Desktop C++ tools, WebView2, and `npm run tauri build`.

## Verification matrix

| Area                  | Automated now                                                                                                                                                                                                       | Required later/external                                                                                                        |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Domain/catalogue      | Dates/provenance, replay/deduplication, conflicts, search, multi-project records, malformed input                                                                                                                   | Migration fixtures spanning every released schema                                                                              |
| Filesystem/metadata   | Canonical-root enforcement, Unicode/spaces, atomic settings/catalogue/manifests, preservation of project assets                                                                                                     | Junction/reparse-point matrix on native Windows and case-sensitive platform matrix                                             |
| Ingest integrity      | Independent SHA-256, collisions, duplicate detection, cancellation cleanup, missing source, simulated corrupt destination                                                                                           | Real SD cards/cameras, disconnects/power loss, full camera trees, sustained large transfers                                    |
| Syncthing             | Deterministic independent-device event replay and conflict retention                                                                                                                                                | Multi-device conflict-copy simulation with actual Syncthing versions and optional REST availability                            |
| Search/catalogue      | Combined text fields and rebuilding from shared events                                                                                                                                                              | SQLite/FTS projection performance, saved compound filters, thousands-of-record benchmarks                                      |
| Searchable UI         | Open/control click, case-insensitive filter, mouse/keyboard selection, clear, Escape, outside click, empty/disabled/loading, multi-select, duplicate names/stable IDs, explicit create, ARIA/focus, App batch flows | Screen-reader passes (Narrator/NVDA), high zoom, Windows WebView2, touch, very large data profiling                            |
| Media formats         | Generated H.264/H.265 FFprobe and thumbnail fixtures when FFmpeg is present                                                                                                                                         | DJI/Sony/professional sample matrices, MOV/WAV/MXF/R3D/BRAW licensing and decoder availability                                 |
| Premiere              | None claimed                                                                                                                                                                                                        | Supported Premiere/UXP versions, imports, bin creation, duplicate reconciliation, open-project + Syncthing scenarios           |
| Archive/deletion      | No destructive feature exists; rules and boundaries are documented                                                                                                                                                  | Complete-source/consolidated archives, playable-project independence, shared references, quarantine, authorization and restore |
| Native/platform       | TypeScript web build and Tauri scaffold inspection                                                                                                                                                                  | Windows signed installer/upgrade/rollback; later Linux packaging and macOS signing/notarization                                |
| Security/dependencies | ESLint, TypeScript, path tests, `npm audit --audit-level=high`, least-privilege capability review                                                                                                                   | Release artifact scan, SBOM/license review, installer signature, adversarial filesystem review                                 |

## Test design rules

- Use synthetic or disposable fixtures; never personal footage.
- Keep destructive tests inside a uniquely created temporary fixture directory and verify its resolved boundary before cleanup.
- Assert outputs and failure state, not only that a call completed.
- Test interruption, replay, duplication, malformed inputs, and collision behavior for every persistent operation.
- Accessibility tests must exercise keyboard behavior and semantics. Manual assistive-technology checks remain necessary before release.
- FFmpeg-dependent tests may skip only with a visible reason. CI installs FFmpeg so those paths execute there.
- Record external application, OS, hardware, and version details for manual evidence.
