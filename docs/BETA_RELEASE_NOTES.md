# Kairix PIMP Light 0.3 — Development

Kairix PIMP Light 0.3 is a public Development prerelease for Windows. It is a copy-only media workflow application: original source removal remains unavailable in ordinary builds.

## What is working

- Responsive metadata-first SD-card and folder inventory with reviewable destination paths, preserved camera-package hierarchy and separate-root collision handling.
- Fast and Deep ingest modes. Both hash the exact protected source handle while writing same-filesystem staging, flush staged bytes and promote without overwrite; Deep additionally rereads and hashes the staged handle.
- Persistent background ingest jobs, interruption recovery, immediate provisional metadata editing and safe completed-history clearing.
- Library browsing, search, sorting, tags, notes, favourites, project assignment and selective integrity verification.
- Logical video/audio recording association while retaining both immutable physical files.
- Shared static thumbnails and hover-scrub derivatives, bounded background preparation, configurable hold frames and responsive thumbnail sizing.
- Clip Viewer playback with validated local proxy generation for unsupported media.
- COPY-only native Windows drag-and-drop to Explorer and compatible hosts, plus manual Premiere manifest export.
- Read-only Premiere project indexing with explicit unknown/historical evidence; no undocumented project-file modification.
- Portable append-only catalogue events and synchronized regenerable derivatives designed for eventual Syncthing delivery.
- Project backup jobs with staged no-overwrite copying, SHA-256 manifests, re-verification and restore testing to an independent empty folder.
- Manual public update discovery through permanent GitHub Releases, including version comparison, release notes, build identity and checksum guidance when signed updating is unavailable.

## Safety and recovery changes

Ingest hashes and file identity now come from the actual open source and staging handles. The staging identity is carried through atomic promotion and checked again before catalogue registration. Duplicate verification uses the same handle-bound reasoning. The isolated cleanup-acceptance build now requires an independently stored app-local fixture trust record, and deletion observation distinguishes confirmed absence from permission, device and other indeterminate failures.

These cleanup corrections do not enable production deletion. The ordinary application contains no executable source-removal path.

## Important limitations

- Production SD-card/source deletion remains disabled pending physical removable-media acceptance and a separate activation review.
- Full automatic project finalization is not implemented.
- Returned-project support does not automatically discover projects or restore archived project metadata; complete archived-project restoration is not implemented.
- AI analysis and Storyteller functionality are not implemented.
- The Premiere companion proof has automated interchange coverage but has not completed broad installed Premiere acceptance.
- Real two-device Syncthing, production-camera/card, large-library installed performance and complete production-codec acceptance remain open.
- Installers are not Authenticode signed and may trigger Windows SmartScreen.
- Signed automatic updating is unavailable unless the protected updater public key and signing secrets are genuinely provisioned and an installed A-to-B update is accepted. Manual release discovery remains available.

Install only from the [official GitHub Releases page](https://github.com/Dubcodes/kairix-pimp/releases). Compare the installer against `SHA256SUMS.txt` and confirm `BUILD-INFO.txt` names the expected source commit before running it.
