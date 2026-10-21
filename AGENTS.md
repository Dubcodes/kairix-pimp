# Kairix PIMP development guidance

- Treat original media as immutable. Never delete, truncate, rewrite, or rename a source file.
- All destination writes use a sibling temporary file, checksum verification, and atomic promotion.
- Reject paths outside configured roots after canonical resolution; do not rely on string prefixes.
- Shared Syncthing data is append-only per device. Never synchronize SQLite or mutate another device's log.
- Keep `packages/core` platform-neutral. OS and filesystem work belongs in an adapter.
- Light must not import AI runtimes or models. Full-edition intelligence is a later optional worker package.
- Do not claim Premiere integration is working without testing it in a supported Premiere version.
- Do not add destructive cleanup in early milestones. Archive and quarantine designs require explicit review.
- Run `npm test`, `npm run lint`, `npm run format`, and `npm run build` before committing.
- Use disposable temporary directories and synthetic media in tests.
