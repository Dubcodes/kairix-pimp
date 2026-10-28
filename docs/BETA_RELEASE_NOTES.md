# Kairix PIMP Light 0.1.1-5 — Windows Beta

This is a public prerelease for Windows x64. Download the plainly named per-user
NSIS Setup executable from the GitHub Release; the MSI is an alternative. Both
installers are unsigned, so Windows may display a SmartScreen warning. Confirm
the source commit in `BUILD-INFO.txt` and verify `SHA256SUMS.txt` before running it.

This build adds conservative completed-ingest history management and bounds the in-memory hover-preview lifecycle. Active, completed and attention-needed jobs are counted separately; only native-revalidated finalized history can be cleared, without touching media, metadata events or recovery evidence. Preview requests use an eight-entry LRU/in-flight bound, mounted cards release frame collections after hover or when leaving the viewport, and displayed timecodes come from FFmpeg's actual sampled timestamps. Native cache clearing waits for FFmpeg generation to release the cache mutex.

## Verified by automated tests and Windows CI

- Recursive source inventory retains common production media, MXF, camera
  sidecars and unknown package files while reporting exclusions and failures.
- Durable background ingest stages outside Master Bin, independently verifies
  SHA-256, prevents overwrite and retains metadata through retry and restart.
- Completed history can be cleared individually or together only after native
  checks reject pending metadata, inventory errors, file failures and recovery journals.
- Batch tags seed eligible video, audio and image metadata without replacing
  later file-specific edits or creating records for camera sidecars.
- The 100-, 700- and 3,000-record thumbnail harness performs one native source
  request per catalogue revision; installed 684-record timing remains to be repeated.
- Preview request tests cover concurrent deduplication, bounded pending and
  completed entries, source-key invalidation, and cache clearing during generation.
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
- Authenticode signing, upgrade across versions and SmartScreen reputation are
  not verified. Automatic updates require a one-time manual bootstrap containing
  the approved updater public key and remain non-operational until a real signed
  higher-version upgrade passes. FFmpeg and FFprobe are optional PATH-based tools and are not bundled.
- Production-media and real two-device Syncthing acceptance remain incomplete.
- Windows x64 is the only published platform for this beta.
- Premiere handoff is manual; automated Premiere integration is not included.
- AI processing, archive consolidation, cloud upload and source cleanup are not
  implemented. PIMP never authorizes deletion of original media; the required
  cleanup safety state machine is design-only in `docs/SOURCE_CLEANUP_DESIGN.md`.

Official website: https://www.dubcodesmedia.com/kairix/kairix-pimp
