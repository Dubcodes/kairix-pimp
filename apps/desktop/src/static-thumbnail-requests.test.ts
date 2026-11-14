import { describe, expect, it, vi } from 'vitest';
import {
  StaticThumbnailBackoffError,
  StaticThumbnailRequestRegistry,
} from './static-thumbnail-requests';

describe('static thumbnail request registry', () => {
  it('deduplicates only concurrent work and retries after a failed backoff', async () => {
    let now = 1_000;
    const registry = new StaticThumbnailRequestRegistry(8, () => now);
    let reject!: (reason: unknown) => void;
    const run = vi.fn(
      () =>
        new Promise<string>((_resolve, rejectRequest) => {
          reject = rejectRequest;
        }),
    );
    const first = registry.request('clip', run);
    const concurrent = registry.request('clip', run);
    expect(run).toHaveBeenCalledTimes(1);
    reject(new Error('FFmpeg failed'));
    await expect(first).rejects.toThrow('FFmpeg failed');
    await expect(concurrent).rejects.toThrow('FFmpeg failed');
    expect(registry.size).toBe(0);
    await expect(
      registry.request('clip', async () => 'not-yet'),
    ).rejects.toBeInstanceOf(StaticThumbnailBackoffError);
    now += 500;
    await expect(
      registry.request('clip', async () => 'ready.jpg'),
    ).resolves.toBe('ready.jpg');
  });

  it('does not retain undefined or a cancelled queued request as success', async () => {
    let now = 1_000;
    const registry = new StaticThumbnailRequestRegistry(8, () => now);
    await expect(
      registry.request('missing', async () => undefined),
    ).rejects.toThrow(/no source/i);
    now += 500;
    await expect(
      registry.request('missing', async () => 'restored.jpg'),
    ).resolves.toBe('restored.jpg');

    await expect(
      registry.request('cancelled', async () => {
        throw new Error('Preview work left the active viewport.');
      }),
    ).rejects.toThrow(/active viewport/);
    await expect(
      registry.request('cancelled', async () => 'visible-again.jpg'),
    ).resolves.toBe('visible-again.jpg');
  });

  it('uses a changed source identity or cache generation as a new key', async () => {
    const registry = new StaticThumbnailRequestRegistry();
    const run = vi.fn(async () => 'ready.jpg');
    await registry.request('epoch-1:media:sha-a', run);
    await registry.request('epoch-1:media:sha-b', run);
    await registry.request('epoch-2:media:sha-b', run);
    expect(run).toHaveBeenCalledTimes(3);
  });
});
