# Provider-neutral verified backup manifest

PIMP's first backup implementation copies an entire registered Working Project
to a separately selected destination. It is deliberately independent of
Syncthing and cloud providers.

Each generation contains an immutable `kairix-pimp-backup-manifest.json` after
all project files have been copied and independently verified. Schema 1 records
a stable backup and project ID, creation time, total file count and byte count,
and each portable project-relative path, byte size and lowercase SHA-256 digest.
`collectionComplete` remains false until a future supported Premiere dependency
collector proves that the project is independently resolvable.

Absolute source/destination paths, volume identity, progress and retry errors are
device-local job state and are not placed in the portable manifest. Credentials,
browser sessions and provider object IDs are never manifest fields.

## External-drive behavior

The native adapter inventories and hashes the source before copying. It rejects
overlapping roots and symbolic-link escapes, identifies the destination volume,
stages files on that volume, flushes and hashes staged bytes, rechecks source
consistency, and atomically promotes without replacing an existing name. A job
is `verified` only after every promoted destination file is read and compared to
the planned size and hash.

Device-local `backup-jobs.json` preserves intent, progress and failure evidence.
Active jobs become `interrupted` after restart and require explicit review and
retry. A retry accepts an existing destination file only when its size and hash
already match the manifest. The restore-test operation requires a separate empty
folder, copies every backed-up asset, and independently verifies all hashes.

No backup state authorizes deletion. The application contains no cleanup command
and does not remove source files, backup generations, staging evidence or cloud
objects.

## Cloud adapter boundary

A provider adapter may consume this manifest, but it must expose honest transfer
and verification states. An upload acknowledgement is not verification. Remote
verification requires an independent download/read of every object followed by
size and SHA-256 comparison. Degoo-specific findings are documented in
[`DEGOO_INTEGRATION.md`](DEGOO_INTEGRATION.md).
