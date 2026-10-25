# Kairix PIMP 0.1.0 beta

This is an unsigned Windows beta for disposable-media testing. Confirm the
source commit in `BUILD-INFO.txt` and verify an installer with
`SHA256SUMS.txt` before running it.

## Verified by automated tests and Windows CI

- Recursive source inventory retains common production media, MXF, camera
  sidecars and unknown package files while reporting exclusions and failures.
- Durable background ingest stages outside Master Bin, independently verifies
  SHA-256, prevents overwrite and retains metadata through retry and restart.
- The Library supports search, desktop-style multiple selection, batch metadata
  changes, Explorer actions and multi-path clipboard handoff to Premiere Pro.
- Portable append-only event replay covers offline delivery, duplicate delivery,
  concurrent additive edits and reported scalar conflicts.
- Windows CI builds nonempty unsigned MSI and NSIS installers from the recorded
  commit after the required TypeScript, Rust, build and audit checks pass.

## Manual or external acceptance still required

- Complete the installed-app checklist in `docs/BETA_TESTING.md`, including a
  same-build reinstall and removable-source interruption.
- Perform a real two-machine Syncthing pass before using production footage.
- Windows signing, upgrade across versions and SmartScreen reputation are not
  verified. FFmpeg and FFprobe are optional PATH-based tools and are not bundled.
- Premiere handoff is manual; automated Premiere integration is not included.
- AI processing, archive consolidation, cloud upload and source cleanup are not
  implemented. PIMP never authorizes deletion of original media.
