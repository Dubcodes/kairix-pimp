import { createRequire } from 'node:module';
import { describe, expect, it } from 'vitest';

const require = createRequire(import.meta.url);
const manifestCore = require('./manifest-core.cjs') as {
  validate(value: unknown): { items: Array<{ mediaId: string }> };
  normalizePath(path: string): string;
  compatibleHostPath(path: string): string;
  groupByBin(
    items: Array<{ binPath: string }>,
    preserve: boolean,
  ): Array<{ binPath: string; items: unknown[] }>;
};

const item = (overrides: Record<string, unknown> = {}) => ({
  mediaId: 'media-a',
  absolutePath: String.raw`C:\Master\20261029\clip.mp4`,
  relativePath: '20261029/clip.mp4',
  originalFilename: 'clip.mp4',
  sha256: 'a'.repeat(64),
  fileType: 'mp4',
  binPath: '20261029',
  ...overrides,
});

describe('Premiere interchange manifest', () => {
  it('accepts a bounded versioned manifest and preserves date bins', () => {
    const manifest = manifestCore.validate({
      schemaVersion: 1,
      kind: 'kairix-pimp-premiere-import',
      createdAt: '2026-10-29T00:00:00Z',
      items: [
        item(),
        item({
          mediaId: 'audio-a',
          absolutePath: 'C:\\Master\\20261029\\clip.wav',
          relativePath: '20261029/clip.wav',
          originalFilename: 'clip.wav',
          fileType: 'wav',
          sha256: 'b'.repeat(64),
        }),
      ],
    });
    expect(manifest.items).toHaveLength(2);
    expect(manifestCore.groupByBin(manifest.items as never, true)).toEqual([
      expect.objectContaining({
        binPath: '20261029',
        items: expect.any(Array),
      }),
    ]);
  });

  it('rejects duplicate, unsafe and malformed media evidence', () => {
    const base = {
      schemaVersion: 1,
      kind: 'kairix-pimp-premiere-import',
      items: [item(), item()],
    };
    expect(() => manifestCore.validate(base)).toThrow(/duplicate/);
    expect(() =>
      manifestCore.validate({
        ...base,
        items: [item({ relativePath: '../clip.mp4' })],
      }),
    ).toThrow(/unsafe/);
    expect(() =>
      manifestCore.validate({
        ...base,
        items: [item({ sha256: 'not-a-hash' })],
      }),
    ).toThrow(/SHA-256/);
    expect(() =>
      manifestCore.validate({
        ...base,
        items: [item({ absolutePath: 'relative/clip.mp4' })],
      }),
    ).toThrow(/non-absolute/);
  });

  it('normalizes Windows paths for duplicate reconciliation', () => {
    expect(manifestCore.normalizePath('C:\\MASTER\\Clip.MP4')).toBe(
      'c:/master/clip.mp4',
    );
    expect(
      manifestCore.normalizePath(
        String.raw`\\?\C:\Master\Unicode カード\Clip 1.MP4`,
      ),
    ).toBe('c:/master/unicode カード/clip 1.mp4');
    expect(
      manifestCore.compatibleHostPath(String.raw`\\server\share\clip 1.mp4`),
    ).toBe(String.raw`\\server\share\clip 1.mp4`);
    expect(
      manifestCore.compatibleHostPath(
        String.raw`\\?\UNC\server\share\clip 1.mp4`,
      ),
    ).toBe(String.raw`\\?\UNC\server\share\clip 1.mp4`);
    const longPath = '\\\\?\\C:\\Master\\' + `${'x'.repeat(260)}\\clip.mp4`;
    expect(manifestCore.compatibleHostPath(longPath)).toBe(longPath);
  });
});
