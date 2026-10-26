# Windows beta installation and upgrades

The recommended beta package is the **NSIS per-user installer**. It installs without elevation under the current Windows profile and preserves PIMP's device-local settings, diagnostic logs, thumbnail cache configuration and recoverable ingest-job history across an upgrade. Shared catalogue records remain in the configured catalogue location and are not installer-owned data.

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
