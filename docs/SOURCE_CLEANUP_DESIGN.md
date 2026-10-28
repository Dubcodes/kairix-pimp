# Verified source cleanup design

Source cleanup is a future, separately authorized operation. The current beta implements **Copy** only and never deletes, renames or rewrites source media. A successfully verified ingest is not by itself permission to clear a card, and a Syncthing replica is not an independent backup.

## Required state model

Cleanup must be a durable phase after the entire requested ingest reaches a terminal verified state. Its journal must contain the ingest job ID, immutable source-root identities, every intended source file, camera-package membership, original canonical path, file identity where the platform exposes one, size, modified time, destination path, destination SHA-256, source SHA-256, eligibility decision, confirmation time and final cleanup result. Restart must resume reporting or revalidation, never assume an attempted removal succeeded.

The verified-copy completion report must list imported, duplicate, excluded, failed, ambiguous and changed files. Cleanup eligibility is false if any required file is excluded, failed, unresolved, retryable, changed or lacks a committed no-overwrite destination whose bytes independently match the recorded SHA-256.

## Safety gates

Before offering cleanup, the host must:

1. Re-open the durable job and prove every intended package member is finalized and registered.
2. Canonically revalidate every source and destination beneath their originally authorized roots without following symlinks, junctions or other reparse-point escapes.
3. Reject a filesystem root, overlapping selected roots, a destination equal to or contained by a source, and a source equal to or contained by the destination or application staging area.
4. Recheck source path, identity, type, size, modified evidence and SHA-256. Any difference retains that source and invalidates its package.
5. Recheck the final committed destination independently, including SHA-256 and catalogue identity. Staged files are never sufficient.
6. Group camera-package recordings and sidecars using the inventory captured before copy. A group is eligible only as a complete unit; folder deletion is never inferred from partial file eligibility.
7. Produce an immutable review showing every exact file proposed for removal and every retained file with its reason.
8. Require a separate explicit confirmation after verification. The warning must state that the Master Bin copy is not an independent backup.

Copy mode can never enter this workflow automatically. A future opt-in mode may prepare the completion report, but it must still stop at the confirmation gate.

## Removal and recovery

On fixed local volumes, a recoverable, application-owned quarantine is preferred when it can remain on the same filesystem and cannot collide with synchronized or source paths. On removable media where recoverable deletion is unavailable, the UI must identify the action as irreversible and require stronger confirmation. The host removes individual validated files only; it never recursively deletes a selected directory or camera folder.

Each successful removal is journaled durably before advancing. A failed or interrupted cleanup retains the remaining files and exposes retry/review actions. Empty directories may be reported for manual cleanup but are not automatically removed in the first implementation. Cleanup history is retained as recovery evidence and is not subject to ordinary finalized-ingest history pruning.

## Required tests before enablement

Use only repository-local or OS-provided synthetic removable-drive fixtures. Cover changed size/time/hash, replaced file identity, symlink and junction escapes, root and overlap rejection, nested selected roots, destination tampering, no-overwrite collisions, incomplete packages, sidecars, excluded files, duplicate destinations, permission failures, disconnects between every journal transition, partial quarantine/removal and restart recovery. Windows installed-app acceptance must review the proposed list and exercise recoverable and irreversible-volume messaging.

Until those native tests and installed-Windows acceptance pass, the product must expose only the verified-copy completion report and this design—not a cleanup control or destructive command.
