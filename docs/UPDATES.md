# Windows beta distribution and signed updates

Kairix PIMP uses three deliberately separate distribution paths.

## Development builds

Pull requests run the complete automated checks and create short-lived, unsigned MSI and NSIS Actions artifacts. These builds never receive the updater public key or private signing material, never publish a GitHub Release and never update the approved Beta manifest.

## Public manual beta releases

An authorized `Publish public Windows beta` dispatch runs only from current `main` and uses the protected `public-beta-release` environment. It repeats the complete checks, verifies package/Cargo/Tauri versions, builds both Windows installers, records SHA-256 checksums and creates a permanent public GitHub prerelease. The plainly named per-user NSIS installer is recommended; the MSI is an alternative.

These installers are not Authenticode signed, so Windows may display a SmartScreen warning. Tauri updater signatures authenticate automatic-update payloads inside PIMP; they are not Windows executable code signatures.

Manual publication does not require private updater credentials. If the repository-level `PIMP_UPDATER_PUBLIC_KEY` variable is absent, the installer remains a valid manual beta but reports automatic updates as unavailable. If the variable is present, packaging validates its Tauri/minisign syntax and embeds that exact public key.

## Approved signed automatic updates

Installed applications check only:

`https://github.com/Dubcodes/kairix-pimp/releases/download/beta-channel/latest.json`

The protected `Publish signed automatic beta update` workflow accepts an existing reviewed prerelease tag. It verifies that the tag commit is reachable from `main`, that the version matches, that nonempty public installers and checksums already exist, and that the configured public key is valid. The `beta-release` environment alone supplies `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

Before signing, the workflow verifies package, desktop, Cargo and Tauri versions; the selected tag's reachability from `main`; and the existing manual installer's source identity and SHA-256 evidence. It builds the official Tauri NSIS updater payload, independently verifies its signature with the embedded public key, refuses to replace an existing versioned signed asset, uploads the payload/signature/checksum, reads the published bytes back, and verifies their hash and signature again. Only then does it update `beta-channel/latest.json`. A failure before that final step cannot advertise a missing or unverified payload. Replacing the fixed GitHub asset may briefly make the endpoint unavailable, which is a fail-closed check error rather than an unsigned or partial update. Pull requests cannot invoke the protected environment or receive signing secrets.

The update client keeps at most one check and one installation attempt in flight. A failed configuration preflight, offline request, malformed manifest or invalid signature clears the pending updater resource. A failed/interrupted download is not reused; the next attempt performs a fresh signed check. The UI reports the Windows installer launch and requires the reopened application version to confirm the restart rather than claiming a restart succeeded.

The native host authorizes installation through one short operation coordinator. Authorization fails while durable ingest work is unresolved, a backup job is active, or a scan, integrity verification, backup/restore verification, FFmpeg/FFprobe enrichment, hover-preview generation/preparation, preview-cache clear, or ingest start/resume/retry is in progress. Once installation is authorized, those operations cannot start. The coordinator mutex is held only while acquiring/releasing permits and checking durable state; it is never held during long filesystem or process work.

## Protected configuration checklist

1. Generate a password-protected pair on a secured operator machine: `npm run tauri -- signer generate --write-keys <secure-private-key-path>`.
2. In repository Actions variables, create `PIMP_UPDATER_PUBLIC_KEY` using the complete base64 public-key value produced by Tauri. Public keys are not secret; keeping it repository-scoped makes the manual bootstrap and later signed payload use the same value.
3. Create GitHub environments `public-beta-release` and `beta-release`. Require an owner review, prevent self-review where the plan supports it, restrict both to `main`, and restrict `beta-release` to authorized maintainers.
4. In `beta-release` only, add secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
5. Never paste private material into commits, issues, workflow inputs, prompts, release assets or logs.
6. Publish a reviewed manual beta from `main`, install it, and confirm Settings → Updates says the signed Beta channel is configured.
7. Merge and publish a higher numeric-prerelease version, then dispatch the signed update workflow for that existing release tag.
8. Download the manifest, payload and signature and independently verify them before installed-app acceptance.

## One-time bootstrap

Builds such as `0.1.1-2` that lack the approved public key cannot securely accept an automatic update. Install the public-key-enabled reviewed NSIS beta manually once. Never weaken signature enforcement or offer an unsigned payload to those builds.

A real higher-version installed-Windows update must then demonstrate check, release notes, progress, signature acceptance, tamper rejection, interrupted download, fresh-check retry, install, restart, version change and preservation of device identity, roots, catalogue and recoverable ingest/backup jobs. It must also exercise the operation gate against active ingest, backup, scan, verification, enrichment and preview work. Until that pass succeeds, describe automatic updates as configured but not operational.

The Tauri configuration declares the fixed endpoint and requires signed versions. The native builder supplies the validated compile-time public key; the runtime configuration command validates the same value before the UI is allowed to make a network check. An empty or malformed key always fails closed.
