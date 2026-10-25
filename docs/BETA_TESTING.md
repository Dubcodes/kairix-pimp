# Windows beta installation and acceptance

Kairix PIMP 0.1.0 is an unsigned internal Windows beta. It is suitable for
disposable generated media only until the real two-device Syncthing acceptance
pass is complete. Do not make it the only copy of production footage.

## Install

1. Open the successful **Windows beta installer** workflow run for the tested
   commit and download its `Kairix-PIMP-...-unsigned-windows` artifact.
2. Extract the artifact and run either the NSIS `setup.exe` or MSI installer.
3. Windows SmartScreen may warn because the beta is not code-signed. Confirm
   the artifact's commit in `BUILD-INFO.txt` before choosing **Run anyway**.
4. WebView2 is included with current supported Windows versions. If Windows
   reports that it is missing, install the Microsoft WebView2 Evergreen runtime.
5. FFmpeg is optional. For thumbnails and FFprobe technical metadata, install
   the supported PATH-based dependency and restart PIMP:

   ```powershell
   winget install Gyan.FFmpeg
   ```

Verified ingest, SHA-256 checks, catalogue browsing, search and metadata edits
remain available when FFmpeg or FFprobe is missing. PIMP displays the affected
optional features at launch.

## Disposable native acceptance checklist

Use newly generated MP4/WAV fixtures and empty directories. Perform this against
the installed application, not `npm run dev`.

- [ ] Fresh install and first launch create one stable device identity.
- [ ] Configure empty **DCM Master Bin** and **Working Projects** directories.
- [ ] Confirm a new installation selects **Use today's date** by default.
- [ ] Drag a fixture folder into Ingest and retain the source fixtures.
- [ ] Review the pre-copy inventory count, bytes, hierarchy-preserved files,
      exclusions and any issues before starting the ingest.
- [ ] Confirm background byte/file progress and independently verified results.
- [ ] While copying, add tags, project assignments, a description and favourite.
- [ ] Close and reopen PIMP; confirm catalogue and job state persist.
- [ ] Interrupt a multi-file job, reopen, reconnect its source and explicitly retry.
- [ ] Re-ingest a duplicate and confirm existing metadata is retained.
- [ ] Search the Library and confirm generated thumbnails when FFmpeg is present.
- [ ] Exercise click, Ctrl+click, Shift+click and Ctrl+A selection; batch add and
      remove tags/projects, set a description and favourite state.
- [ ] Copy multiple CRLF-separated paths for manual Premiere import and confirm
      the right-click actions match the visible selection actions.
- [ ] Confirm **Reveal in Explorer**, **Open date folder** and **Copy path**.
- [ ] Remove FFmpeg/FFprobe from `PATH`, restart, and confirm the warning while
      verified ingest and ordinary catalogue use remain functional.
- [ ] Confirm no source file was renamed, changed or deleted.
- [ ] Safely simulate a removed source and an unavailable/full destination where
      practical; confirm the job fails with a recovery action plus technical detail.

Record the Windows version, installer filename, commit SHA, FFmpeg version and
result of every item. A browser test is not evidence for a native checklist item.

## Syncthing acceptance

Configure **DCM Master Bin itself** as the Syncthing folder on each device. PIMP
does not need a Syncthing REST API or API key. Give each PIMP installation its
own device identity and never copy the device-local PIMP configuration between
computers.

For a disposable two-device pass:

1. Create empty Master Bin mappings on two devices and synchronize them.
2. Ingest generated media on device A, wait for Syncthing, then confirm device B
   reports the file locally available and converges on tags/projects.
3. Make offline metadata edits on both devices, reconnect, and confirm additive
   tags/projects converge and scalar conflicts are reported rather than guessed.
4. Interrupt a transfer and confirm partial files are not catalogued as complete.

Native ingest stages in a hidden sibling directory named
`.DCM Master Bin.pimp-staging-<device>`. That sibling is automatically outside
sync when DCM Master Bin itself is the Syncthing folder. If a broader parent is
synced instead, add this root-relative pattern to that parent's `.stignore`:

```text
/.DCM Master Bin.pimp-staging-*
```

The real two-device result must be recorded before using the beta with production
media.

Automated tests use two independent disposable catalogue roots and exchange only
their owned append-only event logs to simulate offline edits and delayed delivery.
That local simulation is not evidence of a genuine two-machine Syncthing pass.
