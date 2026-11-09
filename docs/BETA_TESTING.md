# Windows beta installation and acceptance

## Current Development candidate 0.2

Current source is internal version `0.2.0`, displayed as `0.2 — Development`. Pull-request artifacts remain unsigned internal verification output; an approved tester build should instead be promoted to a permanent GitHub Development Release. Until that promotion succeeds, `v0.1.1-5` remains the latest public beta. For upgrade acceptance, verify the NSIS Setup against `SHA256SUMS.txt` and `BUILD-INFO.txt`, then confirm device ID, configured roots, catalogue, shared thumbnails, recoverable jobs and device-local settings remain present.

The same run publishes a separately named **Premiere Companion developer source** artifact. It is unpackaged source for UXP Developer Tool, not a `.ccx`. Follow `PREMIERE_INTEGRATION.md` and use only a disposable Premiere project with synthetic MP4/WAV media.

Use the per-user NSIS installer for normal beta acceptance. If an older system-wide MSI installation exists, follow [WINDOWS_INSTALLATION.md](WINDOWS_INSTALLATION.md); verify the new install before explicitly removing the old application entry or shortcut.

Kairix PIMP 0.1.1-5 is an unsigned public prerelease candidate for Windows. It is suitable for
disposable generated media only until the real two-device Syncthing acceptance
pass is complete. Do not make it the only copy of production footage.

## Install

1. Open the permanent public GitHub prerelease for the tested version. Download
   `SHA256SUMS.txt` and the plainly named NSIS Setup executable; use the MSI only
   when specifically testing that alternative.
2. Verify the installer's SHA-256, record the release tag and source commit, then
   run it. Expect a SmartScreen warning because the beta is not Authenticode signed.
3. Confirm the release's `BUILD-INFO.txt` describes the same version and commit
   before choosing **Run anyway**.
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
- [ ] Launch from the installed shortcut and confirm no Command Prompt accompanies PIMP.
- [ ] Confirm normal launch is visible, a second shortcut launch restores the same window, a tray click and **Show PIMP** restore it, and **Quit PIMP** ends the process.
- [ ] With FFmpeg installed, trigger probing/thumbnail work and confirm no console window flashes.
- [ ] Reinstall the same build over the first installation; confirm settings,
      device identity, catalogue and job history remain intact.
- [ ] Configure empty **DCM Master Bin** and **Working Projects** directories.
- [ ] Confirm a new installation selects **Use today's date** by default.
- [ ] Drag a fixture folder into Ingest and retain the source fixtures.
- [ ] Review the pre-copy inventory count, bytes, hierarchy-preserved files,
      exclusions and any issues before starting the ingest.
- [ ] During a large-card review, confirm **Background Operations** advances through enumeration, inspection, checksums and duplicate checks while Library navigation remains responsive; cancel and confirm card reads settle without starting a copy.
- [ ] Start another inventory while one is active and confirm only the newest review completes and no overlapping sustained card reads remain.
- [ ] Confirm background byte/file progress and independently verified results.
- [ ] While copying, add tags, project assignments, a description and favourite.
- [ ] Close and reopen PIMP; confirm catalogue and job state persist.
- [ ] Interrupt a multi-file job, reopen, reconnect its source and explicitly retry.
- [ ] Re-ingest a duplicate and confirm existing metadata is retained.
- [ ] Search the Library and confirm generated thumbnails when FFmpeg is present.
- [ ] Open a disposable approximately 1,000-record Library, record time-to-responsive
      and confirm scrolling/navigation remains responsive while near-visible thumbnails load.
- [ ] After a reviewed large-card inventory, rapidly switch Today, Yesterday and a chosen date. Confirm only the latest plan is active, no new files appear, and Process Monitor shows metadata checks rather than another full source-content read.
- [ ] Hover several local videos, scrub across the frames, move the pointer away
      and confirm the static thumbnail returns without audio or console windows.
- [ ] Open synthetic HEVC footage, generate the H.264/AAC fallback, record processed/total time and throughput, then cancel another conversion and confirm FFmpeg exits before retry. Repeat once while quitting PIMP and check for no orphan process or source change.
- [ ] While the Clip Viewer is open, type K and Space and use Ctrl+Z in descriptions and tag search. Confirm text editing wins; after focus leaves the editor, confirm K/Space controls playback.
- [ ] Index a disposable Premiere project and confirm Background Operations shows the real folder/project/reference stage without inventing a percentage.
- [ ] Exercise thumbnail checkboxes by mouse and keyboard plus click, Ctrl+click,
      Shift+click and Ctrl+A; confirm deselecting one checkbox preserves the rest,
      then batch-edit tags/projects, description and favourite state.
- [ ] Use **Copy file paths** and confirm it produces CRLF-separated text only;
      it must not claim that Premiere imported the media.
- [ ] Confirm **Reveal in Explorer** visibly selects disposable files whose names contain spaces, commas and Unicode, including an attached WAV; repeat with an already-open Explorer window and confirm an offline file reports an error.
- [ ] Remove FFmpeg/FFprobe from `PATH`, restart, and confirm the warning while
      verified ingest and ordinary catalogue use remain functional.
- [ ] Confirm no source file was renamed, changed or deleted.
- [ ] Close PIMP during inventory and during copy, then relaunch; confirm a clean shutdown or recoverable interrupted job with no promoted unverified file. Record any Windows crash/event evidence rather than inferring a cause.
- [ ] Safely simulate a removed source and an unavailable/full destination where
      practical; confirm the job fails with a recovery action plus technical detail.

Record the Windows version, installer filename, commit SHA, FFmpeg version and
result of every item. A browser test is not evidence for a native checklist item.

## Approximately 700–1,000-record responsiveness measurement

This pass is intentionally manual because jsdom timings do not measure WebView2,
native IPC, FFmpeg, Windows scheduling or installed-process memory. Do not point
the test build at a production Master Bin. Use an empty disposable Master Bin,
Working Projects folder and generated media beneath a clearly named test root;
retain the generated source until the pass is complete.

1. Install the exact CI artifact under test and record Windows version, CPU,
   installed RAM, storage type, installer name, commit and FFmpeg version.
2. Populate the disposable catalogue by ingesting 700–1,000 generated
   short video fixtures. Wait for discovery/enrichment to settle, quit from the
   tray, and reopen from the installed shortcut.
3. Start a stopwatch with the shortcut launch. Record time until the main window
   accepts a Settings click and time until the Library responds to a scroll.
4. Open **Settings → Diagnostics** after the catalogue stabilizes. Record
   **Thumbnail source batches**; one catalogue revision should require one batch,
   not one native request per card.
5. In Task Manager, record PIMP CPU and working set at settled idle, peak startup,
   after scrolling through the Library, and after hovering at least 20 different
   videos. Memory may fluctuate but must plateau rather than retain every Base64
   preview; FFmpeg preview generation remains serialized.
   Record pointer-to-first-frame latency separately for a clip already in the RAM
   cache, a clip whose preview exists only on disk after restart, an uncached clip,
   and an uncached clip hovered while optional idle preparation is active. Use the
   same synthetic duration/codec and record values rather than subjective labels.
   Confirm rapid scrubbing remains fluid and only pointer exit persists the final
   last-hovered position.
6. Start a separate disposable ingest of at least 63 files. While SHA-256 copy and
   verification are active, repeatedly scroll, search, open Settings, return to
   Library and hover local videos. Record the longest observed input stall, CPU
   peak and working-set peak. Confirm ingest progress continues and no source is
   changed.
7. While a preview is generating, choose **Clear preview cache**. Confirm the UI
   reports that it is waiting, completes without an error, restores static
   thumbnails and regenerates only when a video is hovered again.
   With optional idle preparation enabled, continuously scroll or move the pointer
   and confirm offscreen generation does not begin until interaction settles.
8. After ingest completion, confirm the header says **All ingests complete**,
   completed history is collapsed, and **Clear Completed** removes the indicator.
   Restart and confirm active/attention history and catalogue media remain intact.

Record raw timings and memory/CPU observations rather than “fast” or “responsive.”
Treat input stalls longer than two seconds, thumbnail batch counts growing per
card, unbounded memory growth, stalled ingest, or FFmpeg/cache-clear errors as a
failed acceptance item. This procedure remains outstanding until run against the
installed PR #6 test build.

## Native outbound-drag acceptance

Use generated disposable MP4 files beneath a disposable configured Master Bin;
record hashes before and after every case. Drag one card into an empty Explorer
folder, then multiple explicitly selected cards and one real Library folder.
Confirm the destination receives copies, the source paths and hashes remain
unchanged, and cancelling over a prohibited target creates nothing. Repeat with
spaces, Unicode and a supported long path, then verify unavailable or changed
catalogue entries are rejected. Finally drag the generated MP4 into a disposable
Premiere Pro 26.0.2 project and confirm Premiere recognizes the original media
without modifying it. Record the Windows/Premiere versions and result. Rust and
jsdom tests do not satisfy this installed cross-process acceptance gate.

The 2026-11-09 correction pass launched a separately identified packaged test
executable, configured repository-local disposable roots, and fully indexed two
generated H.264 MP4 files (including spaces and Unicode) with 2 registered,
0 pending and 0 errors. Their source SHA-256 values were recorded. Windows unit
coverage also queried the real Shell `IDataObject` for `CF_HDROP` with two files,
a Unicode folder and a greater-than-260-character path. Cross-window input is
blocked by the computer-control safety boundary, so these checks do not satisfy
the Explorer or Premiere drop gate; record the manual copy/import and post-drop
source/destination hashes here before changing the feature status.

## Signed automatic-update acceptance

Do not mark automatic updates operational from unit tests or a successful signing
workflow alone. First manually install a reviewed bootstrap release containing the
approved updater public key and confirm Settings → Updates identifies the signed
Beta channel as configured. Publish a higher numeric-prerelease from reviewed
`main`, then use a disposable Windows profile to record:

- no-update and newer-update checks, displayed release notes and last-check time;
- download progress, valid-signature acceptance and invalid-signature rejection;
- interrupted download, failed installation, retry, restart and the new version;
- preserved device identity, Master Bin/Working Projects roots, catalogue,
  preferences, completed history and recoverable ingest jobs;
- refusal to install while any ingest is active or recoverable.

Download the versioned payload, signature and `latest.json` separately and verify
their version, URL, size, SHA-256 and minisign signature. This acceptance remains
blocked until the repository owner provisions the protected signing configuration.

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
