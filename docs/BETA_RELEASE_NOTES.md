# Kairix PIMP Light 0.2 — Development candidate

This file supplies release notes to the protected promotion workflows. Source version `0.2.0` is displayed as `0.2 — Development`; it is not publicly available until a permanent GitHub Development Release has completed verification. The historical `v0.1.1-5` beta remains unchanged.

## Highlights

- Approved user builds are promoted as permanent GitHub Releases with NSIS/MSI installers, SHA-256 evidence, source identity and an explicit Development or Stable classification.
- Release and updater publication are serialized; immutable signed assets are verified on retry and channel manifests reject rollback or same-version replacement.
- Settings can select Development or Stable signed update channels and can check after startup, every six hours, or manually. Update installation still fails closed without the protected public key and signature.
- Static thumbnails use deterministic portable identities beneath the shared catalogue at `.pimp/thumbnails/v1`. Valid legacy local thumbnails migrate incrementally; hover previews and playback proxies remain local.
- Static asset references survive card unmount and ordinary catalogue refresh so returning to earlier recordings does not deliberately restore placeholders or rerun FFmpeg.
- Library overflow belongs only to the folder and media panels. Other workspaces retain conditional natural scrolling.

## Safety retained

- Original media is immutable and ingest retains independent SHA-256 verification, same-filesystem staging and atomic no-overwrite promotion.
- Shared thumbnails are regenerable derivatives, not authoritative catalogue identity, and ordinary local cache clearing does not delete them.
- Automatic updates cannot start while critical ingest, backup, verification or derivative filesystem work is active.
- No source cleanup or automatic deletion is enabled.

## Acceptance still required

- Provision protected signing credentials/public key and complete a real installed A-to-B update, tamper rejection and interrupted-download retry.
- Repeat installed Windows scrolling with at least 1,000 logical recordings and test narrow, 1080p, ultrawide and 125–200% scaling.
- Complete real two-device Syncthing thumbnail arrival, concurrent-generation/conflict and different-root testing.
- Complete physical-card, production-codec, Premiere host and Authenticode acceptance.

Install only from the [official GitHub Releases page](https://github.com/Dubcodes/kairix-pimp/releases), compare `SHA256SUMS.txt` and `BUILD-INFO.txt`, and expect a SmartScreen warning while installers remain unsigned by Authenticode.
