# Architecture

## Decision summary

PIMP uses Tauri 2 + React/TypeScript with a small Rust host. Pure TypeScript domain code contains no browser, OS, Tauri, or AI dependencies. Platform adapters implement filesystem, process, FFmpeg, device discovery, and optional Syncthing status. This keeps business rules testable today and keeps future Windows, Linux, and macOS ports narrow.

Tauri was selected over Electron for a smaller runtime distribution and explicit capability boundaries. React provides mature accessible desktop-style UI composition. Rust is reserved for trusted host operations and long-running job orchestration; compute tools such as FFmpeg remain child processes with explicit argument arrays. A Node adapter currently provides the verified reference behavior and test harness while the local Rust/MSVC prerequisites are unavailable.

## Components

```text
React UI
  -> application commands / progress events
  -> trusted host adapter (Tauri Rust; Node reference adapter in tests)
      -> portable core rules
      -> Master Bin files + append-only metadata
      -> local disposable SQLite index (planned)
      -> FFmpeg / FFprobe
      -> optional Syncthing read-only status
      -> optional AI worker protocol (Full only)
```

The frontend never receives broad arbitrary-filesystem permission. A host command accepts relative paths or user-selected handles, resolves canonical paths, enforces configured roots, and reports structured progress.

## Ingest transaction

1. Snapshot source metadata and select capture date with provenance.
2. Compute the destination under `media/YYYY-MM-DD`.
3. Reject an existing different file; recognize an identical hash as a duplicate.
4. Hash source, stream into a unique `.pimp-part-*` file, hash that file independently, compare, and atomically rename.
5. Append a `media.registered` event only after promotion.
6. Probe and preview in retryable background jobs; these may fail without invalidating ingest.

Directory ingest will preserve critical relative subtrees. A resumable job journal and source-volume identity are the next ingest increment.

## Cross-platform boundaries

Portable records contain `/`-separated relative paths and UUID/content identities, never absolute paths. Device configuration maps those roots to local paths. Case-folded comparison is used only to detect hazards on case-insensitive targets; canonical spelling is preserved. OS integrations live behind adapters.

## AI boundary

`AiProcessor` is an optional streaming interface producing timestamped, versioned, provenance-bearing results. Jobs must support pause, cancellation, retry policy, resource limits, cache keys, and model selection. Light reads existing results but ships no models, CUDA/ML frameworks, or inference engines.
