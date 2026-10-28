# Signed Beta updates

Kairix PIMP uses the official Tauri 2 updater. The installed application checks only the Beta channel manifest at:

`https://github.com/Dubcodes/kairix-pimp/releases/download/beta-channel/latest.json`

The updater compares semantic versions, downloads through HTTPS and verifies the artifact's Tauri/minisign signature against the public key embedded at build time. A native operation gate atomically rejects installation while an ingest is queued, copying, verifying, paused or interrupted, and rejects new start/resume/retry commands while installation owns that gate. The gate is released after a failed or interrupted download so the same verified update can be retried. Invalid signatures never reach the installer. The versioned NSIS installer on GitHub Releases remains the manual fallback.

## One-time protected configuration

Updater signing is intentionally fail-closed until the release owner provisions a key pair. Perform these steps on a secured operator machine; the commands below never require printing the private key:

1. Generate a password-protected pair outside the repository: `npm run tauri -- signer generate --write-keys <secure-private-key-path>`. Enter a strong password at the prompt. The CLI writes the private key at the requested path and the matching `.pub` file beside it.
2. Create a GitHub Actions environment named `beta-release`, enable required reviewers and prevent self-review. Configure tag protection/rules so only approved maintainers can create numeric-prerelease tags matching `v*-*` (for example `v0.1.1-2`).
3. In that protected environment, store the complete private-key file contents as the secret `TAURI_SIGNING_PRIVATE_KEY` and the password as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Do not use a command that echoes either value into a terminal transcript or Actions log.
4. Store the Tauri CLI's base64 public-key output (normally beginning with `dW50`) as the environment variable `PIMP_UPDATER_PUBLIC_KEY`. This is the base64 wrapper around the complete minisign public-key text, not merely its inner line beginning with `R`. The application decodes and parses it at runtime and reports an invalid value as unconfigured.
5. From a reviewed clean commit, create the exact tag `v<package-version>`. The protected job builds the signed NSIS updater artifact, creates `latest.json`, publishes the versioned prerelease and updates only the fixed `beta-channel` manifest asset.
6. Download `latest.json`, the `.nsis.zip` payload and its `.sig` from the versioned prerelease. Confirm the manifest URL and version match that release. For independent verification with the `minisign` CLI, base64-decode the `.sig` file into a temporary minisign signature file, base64-decode `PIMP_UPDATER_PUBLIC_KEY` into a temporary public-key file, then run `minisign -Vm <payload.nsis.zip> -x <decoded-signature> -p <decoded-public-key>`. The command must report a valid signature. Delete the temporary decoded files afterward; they contain no private material. Never substitute an Actions PR artifact for the release payload.

If either key setting is absent, CI still publishes the ordinary manual installers but does not create updater artifacts or a manifest. The app reports that signed updates are not configured; it never reports “up to date” without a completed check.

Private signing material must never be committed, written to build logs, attached as an artifact or used for pull-request builds. Pull-request Actions artifacts are not an update source.

## Bootstrap from an older unsigned beta

An installed build without the embedded public key cannot authenticate a newly published update and cannot be upgraded merely by adding a GitHub Release. Bootstrap automatic updates as follows:

1. Provision and validate the protected configuration above.
2. Publish the reviewed `0.1.1-3` per-user NSIS installer manually; it must contain the matching public key.
3. Verify its SHA-256 checksum, install it over the earlier per-user copy, launch it and confirm Settings reports the Beta channel configured.
4. Publish a subsequently higher numeric-prerelease version through the protected signed-update job.
5. On a disposable Windows profile, exercise check, download, install and restart, then confirm the device identity, paths, catalogue, preferences, thumbnail cache and recoverable jobs survived.

Until that end-to-end pass succeeds, automatic updates remain unverified and the reviewed NSIS installer remains the supported route.

## Release acceptance

- Confirm the tagged version is higher than the installed version.
- Verify the manifest platform is `windows-x86_64-nsis` and its URL points to the same reviewed versioned prerelease.
- Exercise check, download, interrupted download, invalid-signature rejection, install and restart on a disposable Windows beta installation.
- Confirm active ingest prevents installation and that settings, catalogue and recoverable jobs survive the upgrade.
- Retain the versioned per-user NSIS installer and SHA-256 file for manual rollback/recovery.
