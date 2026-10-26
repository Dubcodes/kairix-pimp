# Signed Beta updates

Kairix PIMP uses the official Tauri 2 updater. The installed application checks only the Beta channel manifest at:

`https://github.com/Dubcodes/kairix-pimp/releases/download/beta-channel/latest.json`

The updater compares semantic versions, downloads through HTTPS and verifies the artifact's Tauri/minisign signature against the public key embedded at build time. The UI blocks installation while ingest jobs are active. Failed or interrupted downloads do not bypass verification, execute an arbitrary artifact or alter catalogue/job data. The versioned NSIS installer on GitHub Releases remains the manual fallback.

## One-time protected configuration

Updater signing is intentionally fail-closed until the release owner provisions a key pair:

1. Generate a Tauri updater signing key with the official Tauri CLI in a secure environment.
2. Store the complete private key only in the protected GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY` and its password in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Store the corresponding public key in the repository/environment variable `PIMP_UPDATER_PUBLIC_KEY`. It is injected into release builds; it is not secret, but it must match the protected private key.
4. Protect the release environment and require review for version tags matching `v*-beta.*`.
5. Run a reviewed tag build. CI builds the signed NSIS updater package, creates `latest.json`, publishes the versioned prerelease, and updates only the fixed `beta-channel` manifest asset.

If either key setting is absent, CI still publishes the ordinary manual installers but does not create updater artifacts or a manifest. The app reports that signed updates are not configured; it never reports “up to date” without a completed check.

Private signing material must never be committed, written to build logs, attached as an artifact or used for pull-request builds. Pull-request Actions artifacts are not an update source.

## Release acceptance

- Confirm the tagged version is higher than the installed version.
- Verify the manifest platform is `windows-x86_64-nsis` and its URL points to the same reviewed versioned prerelease.
- Exercise check, download, interrupted download, invalid-signature rejection, install and restart on a disposable Windows beta installation.
- Confirm active ingest prevents installation and that settings, catalogue and recoverable jobs survive the upgrade.
- Retain the versioned per-user NSIS installer and SHA-256 file for manual rollback/recovery.
