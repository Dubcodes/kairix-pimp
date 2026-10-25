# Future provider-neutral backup manifest

Cloud backup automation, including Degoo integration, is deliberately deferred.
Nothing in current PIMP ingest, catalogue, metadata or Syncthing behavior depends
on a cloud provider or unofficial API.

A future backup adapter should consume a versioned, provider-neutral manifest.
At minimum each immutable file entry records its portable Master Bin-relative
path, byte size and lowercase SHA-256 digest:

```json
{
  "schemaVersion": 1,
  "manifestId": "018f0000-0000-7000-8000-000000000001",
  "createdAt": "2026-10-25T00:00:00Z",
  "masterBinId": "stable-library-id",
  "files": [
    {
      "relativePath": "20261025/CAMERA_001.MP4",
      "sizeBytes": 1048576,
      "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    }
  ]
}
```

Absolute device paths, credentials, provider object IDs and mutable upload state
must remain outside the portable manifest. A future implementation must validate
canonical paths, verify remote bytes independently and never treat a Syncthing
replica as an independent backup.

## Manual backup for this beta

PIMP does not upload to Degoo or any other cloud provider. After Premiere Project
Manager has collected a completed project into DCM Content, close Premiere and
PIMP, then copy that intact project folder with an existing backup tool to an
external drive and, if desired, to Degoo. Verify the copied file counts and sizes;
for critical work, independently hash the backup or use a backup product that
performs verification. Keep the collected `.prproj` and its media together.

Starting an upload, seeing an upload progress indicator, or receiving a provider
success message never authorizes deletion from Master Bin or DCM Content. This
beta implements no automatic source deletion or cleanup.
