# Windows releases and signed updates

Kairix PIMP separates internal CI builds from approved public releases. Pull requests may create short-lived unsigned MSI and NSIS artifacts for verification, but anything offered to users must be promoted as a permanent [GitHub Release](https://github.com/Dubcodes/kairix-pimp/releases). The historical public beta remains `v0.1.1-5`; source version `0.2.0` is the next Development line and is not public until the protected promotion succeeds.

## Version and classification policy

Repository metadata uses three-component SemVer. The UI hides a zero patch (`1.22.0` is shown as `1.22`), and components are compared as integers, so `1.22` is newer than `1.3`.

- Stable milestones use a whole major number such as `1.0.0` or `2.0.0` and require an explicit protected Stable approval.
- Intermediate releases such as `1.1.0`, `1.2.0`, and the current transitional `0.2.0` are Development releases.
- Numeric shape never promotes a build by itself. A release carries an explicit `Development` or `Stable` classification.
- Legacy prerelease versions such as `0.1.1-5` remain parseable for upgrade comparison.

`npm run release:validate -- <version> <classification>` verifies package, desktop, Cargo and Tauri metadata plus the classification rules.

## Public promotion

The protected **Publish approved public Windows release** workflow accepts an exact version and classification, runs the full JavaScript and Rust verification matrix, verifies that the selected source is current `main`, builds both installers, checks package identity and SHA-256 evidence, and creates a draft release before read-back verification. It refuses an existing tag or release. A Development promotion becomes a prerelease; Stable additionally requires the exact `APPROVE STABLE <version>` phrase and the `stable-release` protected environment. No pull-request workflow receives signing secrets.

Public release assets include the recommended plainly named per-user NSIS installer, MSI alternative, `SHA256SUMS.txt`, `BUILD-INFO.txt`, release notes and classification. Installers are currently not Authenticode signed, so Windows SmartScreen may warn. Tauri updater signatures authenticate updater payloads; they are distinct from Windows code signing.

## Development and Stable channels

Installed builds select a device-local channel:

- **Development** follows `development-channel` and may receive approved Development releases or a newer Stable milestone.
- **Stable** follows `stable-channel` and rejects a manifest classified Development.

The Tauri endpoint is `https://github.com/Dubcodes/kairix-pimp/releases/download/{{target}}/latest.json`; the native host supplies the selected channel target only after validating the embedded public key. Channel changes never bypass Tauri version ordering and therefore cannot downgrade the application. Existing settings default conservatively to Development because all current testers are on development-era builds.

When enabled, PIMP checks shortly after startup and every six hours, with a manual check always available. Offline failures, timeouts, malformed classifications and missing signing configuration fail closed without blocking normal use. The update client keeps one check and one installation attempt in flight. A failed or interrupted download is discarded and must be checked and signature-verified again.

The protected **Publish signed automatic update** workflow runs for an approved published release or an explicit existing tag. It verifies source reachability, version/classification agreement, public installer checksums and build identity before signing. It refuses to overwrite versioned updater assets, downloads them again after publication, compares SHA-256 and independently verifies the signature, then promotes the channel manifest last. Stable promotion also advances Development to that same newer Stable release.

The native operation gate refuses installation while ingest, backup, verification, discovery, enrichment or derivative filesystem work is active. PIMP does not claim success until the updater hands off to the installer; the reopened version remains the confirmation boundary.

## Owner configuration checklist

1. Generate a password-protected Tauri signing pair on a secured operator machine: `npm run tauri -- signer generate --write-keys <secure-private-key-path>`.
2. Add repository variable `PIMP_UPDATER_PUBLIC_KEY` with the complete public-key value.
3. Configure protected environments `public-beta-release`, `stable-release`, and `beta-release`; require owner review, prevent self-review where available, and restrict deployments to trusted `main` revisions.
4. Add `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` only to `beta-release`.
5. Never place private material in Git, workflow inputs, issues, prompts, release assets or logs.
6. Promote and manually install the first reviewed public-key-enabled NSIS build. Keyless legacy installations cannot authenticate an update and require this one-time bootstrap.
7. Promote a higher signed Development release and complete the installed A-to-B acceptance matrix, including tamper rejection, interrupted-download retry, critical-operation refusal, restart, and preservation of settings, device ID, catalogue and recoverable jobs.

The code and protected workflows are ready for owner configuration, but no signing credentials were available in this milestone and no installed signed upgrade was performed. Automatic updating must remain described as unverified operationally until that acceptance pass succeeds.
