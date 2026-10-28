# Kairix PIMP 0.1.1-3 (Beta release candidate)

This is an unsigned Windows beta for disposable-media testing. Confirm the
source and build/test commits in `BUILD-INFO.txt` and verify an installer with
`SHA256SUMS.txt` before running it.

This build incorporates installed-app feedback: planned ingest paths use a responsive bounded table; thumbnail startup resolves validated cache paths once per catalogue revision and lazily decodes near-visible images; hover previews use a bounded fingerprinted local cache; project inventory/navigation rejects stale results; and batch tags persist with ingest jobs. Unconfigured automatic updates now remain in an explicit unavailable state without a network check, with test builds, Releases and the official website exposed as separate fixed links. In-app installation remains unavailable until the protected signing configuration in `docs/UPDATES.md` is provisioned.

## Verified by automated tests and Windows CI

- Recursive source inventory retains common production media, MXF, camera
  sidecars and unknown package files while reporting exclusions and failures.
- Durable background ingest stages outside Master Bin, independently verifies
  SHA-256, prevents overwrite and retains metadata through retry and restart.
- Batch tags seed eligible video, audio and image metadata without replacing
  later file-specific edits or creating records for camera sidecars.
- The 100-, 700- and 3,000-record thumbnail harness performs one native source
  request per catalogue revision; installed 684-record timing remains to be repeated.
- The Library supports search, desktop-style multiple selection, batch metadata
  changes, Explorer actions and multi-path clipboard handoff to Premiere Pro.
- Portable append-only event replay covers offline delivery, duplicate delivery,
  concurrent additive edits and reported scalar conflicts.
- Windows CI builds nonempty unsigned MSI and NSIS installers from the recorded
  commit after the required TypeScript, Rust, build and audit checks pass, and
  inspects the packaged executable for the Windows GUI subsystem.

## Manual or external acceptance still required

- Complete the installed-app checklist in `docs/BETA_TESTING.md`, including a
  same-build reinstall, console/focus/tray/Explorer checks and removable-source interruption.
- Perform a real two-machine Syncthing pass before using production footage.
- Windows signing, upgrade across versions and SmartScreen reputation are not
  verified. FFmpeg and FFprobe are optional PATH-based tools and are not bundled.
- Premiere handoff is manual; automated Premiere integration is not included.
- AI processing, archive consolidation, cloud upload and source cleanup are not
  implemented. PIMP never authorizes deletion of original media; the required
  cleanup safety state machine is design-only in `docs/SOURCE_CLEANUP_DESIGN.md`.
