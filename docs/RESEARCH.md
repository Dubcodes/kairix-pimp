# Research notes

Research was performed against the linked primary documentation on 22 October 2026. These are patterns and constraints, not copied proprietary implementations.

## Ingest and catalogue products

- Hedge OffShoot always verifies transfers and distinguishes fast size-based transfer verification from independent source/destination checksum modes. PIMP adopts the stronger independent hash comparison before catalogue registration.
- Silverstack separates copy and verify phases, preserves user metadata outside originals, registers file copies in a library, and exposes conflicts before metadata import. This supports PIMP's immutable originals, distinct technical/manual fields, job visibility, and explicit reconciliation.
- Video Hub App first captures technical information, then generates thumbnails, keeping browsing useful while previews continue. Its combined filename/folder/tag/notes filters and batch tags validate search-first, background-preview UX.
- TagSpaces' sidecar approach is directionally useful, but a mutable sidecar per item would still conflict under Syncthing. PIMP therefore chooses per-device append-only operation logs and derived item views.

## Syncthing

The local REST API uses an API key and exposes system status, per-folder database status, and a long-poll event stream. The API is explicitly subject to change; status integration must be optional and read-only. Event IDs can have gaps, so PIMP must treat them as hints and rescan its own shared metadata after discontinuities.

## Premiere and archive

Premiere UXP is the documented modern extension platform. Current APIs expose active project access, `Project.importFiles`, root/insertion bins, `FolderItem` actions, and project-item media paths. The integration should reconcile by canonical path/content identity and use documented transactions; `.prproj` parsing is excluded.

Premiere Project Manager can collect files or consolidate/transcode with handles and exclude unused clips. PlumePack advertises lossless trimming for supported codecs, path replacement, checks, and reports, but also documents codec limits and no resume after failure. Both are candidate backends, never a reason to skip PIMP's independent archive validation.

Long-GOP "lossless" trimming can be constrained by keyframe/GOP boundaries and container/tool behavior. A generated or transcoded asset cannot be validated against the source hash; it needs its own hash, derivation recipe, duration/timecode checks, and playable-project validation.

## AI

whisper.cpp exposes segment and experimental token timestamps in a compact C/C++ implementation. faster-whisper exposes word timestamps, batching, VAD, CPU/GPU compute choices, and lazy execution, but carries Python/CTranslate2 packaging implications. Both fit a process-isolated worker contract. Visual-language and audio classifiers remain an experiment because model size, hardware support, licenses, uncertainty calibration, and cross-platform packaging must be measured before selection.

## Tauri and FFmpeg

Tauri 2 capability files can restrict window/webview permissions. PIMP should expose purpose-built commands rather than broad filesystem access. FFprobe JSON is the technical metadata interchange and FFmpeg generates thumbnails/previews as cancellable background jobs using explicit argument arrays.

## Sources

- [Hedge OffShoot verification](https://docs.hedge.video/offshoot/features/verification)
- [Pomfort Silverstack copy process](https://kb.pomfort.com/silverstack/offloadandbackup/how-does-silverstacks-copy-process-work/)
- [Pomfort Silverstack library metadata](https://kb.pomfort.com/silverstack/hands-on/editing-metadata/the-silverstack-library/)
- [Video Hub App guide](https://videohubapp.com/en/guide/)
- [TagSpaces documentation](https://docs.tagspaces.org/)
- [Syncthing REST API](https://docs.syncthing.net/dev/rest.html)
- [Syncthing events](https://docs.syncthing.net/rest/events-get/)
- [Premiere UXP](https://developer.adobe.com/premiere-pro/uxp/)
- [Premiere Project class](https://developer.adobe.com/premiere-pro/uxp/ppro-reference/classes/project)
- [Premiere FolderItem class](https://developer.adobe.com/premiere-pro/uxp/ppro_reference/classes/folderitem/)
- [Premiere Project Manager](https://helpx.adobe.com/premiere/desktop/organize-media/transfer-files/copy-transcode-or-archive-project.html)
- [PlumePack overview](https://www.autokroma.com/PlumePack/)
- [Tauri 2](https://v2.tauri.app/)
- [FFmpeg documentation](https://ffmpeg.org/documentation.html)
- [whisper.cpp](https://github.com/ggml-org/whisper.cpp)
- [faster-whisper](https://github.com/SYSTRAN/faster-whisper)
