# Windows beta installation and upgrades

Download approved builds from the permanent [Kairix PIMP GitHub Releases page](https://github.com/Dubcodes/kairix-pimp/releases). The historical public beta is `v0.1.1-5`; do not assume source version `0.2` is available until its Development Release appears there. The recommended package is the plainly named **NSIS per-user Setup executable**. It installs without elevation under the current Windows profile and preserves device settings, identity, recoverable jobs and local caches. Shared catalogue records and `.pimp/thumbnails/v1` derivatives remain in the configured catalogue location and are not installer-owned data. Verify the installer against `SHA256SUMS.txt` and `BUILD-INFO.txt` before running it.

The beta installers are not Authenticode signed. Windows may show a SmartScreen warning; confirm the publisher warning, version, checksum and GitHub Release source rather than downloading a copy from elsewhere.

The installed release executable is a Windows GUI application and should not open an accompanying Command Prompt; FFmpeg and FFprobe background work should not flash one either. If launch or window restoration fails, use the tray menu when available and inspect Settings → Diagnostics rather than closing an unrelated console window.

## Migrating from an older system-wide installation

Some early beta testing installed both a per-user copy and a system-wide MSI copy. PIMP does not silently remove either installation or delete either profile's data.

1. Finish or pause active ingest work and quit PIMP from its notification-area menu.
2. In Windows **Installed apps**, note any entries for Kairix PIMP and their install scope. Keep the existing application installed until the new NSIS build has launched and displayed the expected Master Bin, catalogue and recoverable jobs.
3. Install the reviewed per-user NSIS beta for the same Windows account. Do not use an unapproved pull-request artifact as an update.
4. Launch the per-user copy and verify Settings → Library & Storage and Diagnostics. Device data under the user's application-data folder is retained; PIMP never relocates the shared catalogue automatically.
5. Right-click each Start-menu or desktop shortcut, choose **Properties**, and compare **Target**. The supported per-user target is beneath the current account's `AppData\\Local\\Programs\\KairixPIMPBeta` directory; a target beneath `Program Files` belongs to the older system-wide MSI.
6. Only after the per-user build has preserved the device identity, Master Bin and catalogue paths, ingest-job history, preferences, thumbnails and diagnostics, explicitly uninstall the older system-wide MSI entry from Windows Settings. Unpin or delete only the shortcut whose target points to that removed MSI. Do not remove either application-data directory or the Master Bin.

The installers do not silently uninstall another scope because that could require elevation, target the wrong Windows account or remove the user's only working fallback. MSI remains a CI compatibility artifact during beta; NSIS per-user is the supported acceptance-test path.

## Automatic-update bootstrap

Older builds without the approved updater public key cannot authenticate an update. Install a reviewed public-key-enabled NSIS release manually once. Settings → Updates must then identify the selected signed Development or Stable channel as configured before a later higher version can be offered. Never use an unsigned Actions artifact as an automatic-update payload. See [Windows releases and signed updates](UPDATES.md).
