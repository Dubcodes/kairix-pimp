import { describe, expect, it, vi } from 'vitest';
import type { HoverPreview } from './bridge';
import { HoverPreviewRequestCache } from './hover-preview-cache';

const preview = (mediaId: string): HoverPreview => ({
  mediaId,
  durationSeconds: 10,
  frameTimes: [0],
  frames: [`data:image/jpeg;base64,${mediaId}`],
});

describe('hover preview request cache', () => {
  it('deduplicates concurrent requests and retains only a bounded LRU', async () => {
    const cache = new HoverPreviewRequestCache(3);
    const loader = vi.fn(async () => preview('one'));
    const first = cache.request('one', loader);
    const duplicate = cache.request('one', loader);
    expect(first).toBe(duplicate);
    await first;
    expect(loader).toHaveBeenCalledTimes(1);

    await cache.request('two', async () => preview('two'));
    await cache.request('three', async () => preview('three'));
    await cache.request('four', async () => preview('four'));
    expect(cache.size).toBe(3);
    await cache.request('one', loader);
    expect(loader).toHaveBeenCalledTimes(2);
  });

  it('bounds pending work during rapid movement across many cards', async () => {
    const cache = new HoverPreviewRequestCache(2);
    let resolveFirst!: (value: HoverPreview) => void;
    let resolveSecond!: (value: HoverPreview) => void;
    const first = cache.request(
      'one',
      () => new Promise((resolve) => (resolveFirst = resolve)),
    );
    const second = cache.request(
      'two',
      () => new Promise((resolve) => (resolveSecond = resolve)),
    );
    await expect(
      cache.request('three', async () => preview('three')),
    ).rejects.toThrow(/queue is busy/);
    expect(cache.size).toBe(2);
    resolveFirst(preview('one'));
    resolveSecond(preview('two'));
    await Promise.all([first, second]);
  });

  it('invalidates in-flight and completed frame data after cache clearing', async () => {
    const cache = new HoverPreviewRequestCache(2);
    let resolve!: (value: HoverPreview) => void;
    const request = cache.request(
      'one',
      () => new Promise((complete) => (resolve = complete)),
    );
    cache.invalidate();
    expect(cache.size).toBe(0);
    resolve(preview('one'));
    await expect(request).rejects.toThrow(/cache was cleared/);

    await expect(
      cache.request('one:changed-source', async () => preview('one')),
    ).resolves.toMatchObject({ mediaId: 'one' });
  });
});
