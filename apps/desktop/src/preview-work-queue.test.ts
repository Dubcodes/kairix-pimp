import { describe, expect, it, vi } from 'vitest';
import { PreviewWorkQueue } from './preview-work-queue';

describe('preview work queue', () => {
  it('runs higher-priority queued work first within one lane', async () => {
    const queue = new PreviewWorkQueue();
    const order: string[] = [];
    let release!: () => void;
    const active = queue.schedule(
      'active',
      4,
      'nearby',
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    const idle = queue.schedule('idle', 5, 'idle', async () => {
      order.push('idle');
    });
    const automatic = queue.schedule('automatic', 3, 'automatic', async () => {
      order.push('automatic');
    });
    const hover = queue.schedule('hover', 2, 'hover', async () => {
      order.push('hover');
    });
    const nearbyStatic = queue.schedule(
      'static-nearby',
      1,
      'static nearby',
      async () => {
        order.push('static-nearby');
      },
    );
    const visibleStatic = queue.schedule(
      'static-visible',
      0,
      'static visible',
      async () => {
        order.push('static-visible');
      },
    );
    release();
    await Promise.all([
      active,
      idle,
      automatic,
      hover,
      nearbyStatic,
      visibleStatic,
    ]);
    expect(order).toEqual([
      'static-visible',
      'static-nearby',
      'hover',
      'automatic',
      'idle',
    ]);
  });

  it('allows a static lane to progress while hover generation is blocked', async () => {
    const hoverQueue = new PreviewWorkQueue();
    const staticQueue = new PreviewWorkQueue();
    let releaseHover!: () => void;
    const hover = hoverQueue.schedule(
      'hover',
      0,
      'hover',
      () => new Promise<void>((resolve) => (releaseHover = resolve)),
    );
    const staticRun = vi.fn(async () => 'thumbnail');

    await expect(
      staticQueue.schedule('static', 0, 'static', staticRun),
    ).resolves.toBe('thumbnail');
    expect(staticRun).toHaveBeenCalledOnce();

    releaseHover();
    await hover;
  });

  it('deduplicates a key and bounds queued low-priority work', async () => {
    const queue = new PreviewWorkQueue(2);
    let release!: () => void;
    const active = queue.schedule(
      'active',
      2,
      'active',
      () => new Promise<void>((resolve) => (release = resolve)),
    );
    const run = vi.fn(async () => 'ready');
    const first = queue.schedule('same', 3, 'idle', run);
    const upgraded = queue.schedule('same', 0, 'hover', run);
    const dropped = queue.schedule('low', 3, 'idle', async () => undefined);
    await expect(
      queue.schedule('another-low', 3, 'idle', async () => undefined),
    ).rejects.toThrow(/queue is full/);
    release();
    await expect(
      Promise.all([active, first, upgraded, dropped]),
    ).resolves.toBeDefined();
    expect(run).toHaveBeenCalledTimes(1);
  });

  it('promotes an already queued visible load when the user hovers it', async () => {
    const queue = new PreviewWorkQueue();
    const order: string[] = [];
    let release!: () => void;
    const active = queue.schedule(
      'active',
      2,
      'active',
      () => new Promise<void>((resolve) => (release = resolve)),
    );
    const load = queue.schedule('load:clip', 1, 'visible', async () => {
      order.push('clip');
    });
    const other = queue.schedule('other', 0, 'other hover', async () => {
      order.push('other');
    });
    queue.promote('load:clip', 0);
    release();
    await Promise.all([active, load, other]);
    expect(order).toEqual(['clip', 'other']);
  });

  it('cancels queued work that leaves the active viewport', async () => {
    const queue = new PreviewWorkQueue();
    let release!: () => void;
    const active = queue.schedule(
      'active',
      0,
      'active',
      () => new Promise<void>((resolve) => (release = resolve)),
    );
    const queued = queue.schedule('static:clip', 0, 'static', async () => {});
    expect(queue.cancel('static:clip')).toBe(true);
    await expect(queued).rejects.toThrow(/active viewport/);
    release();
    await active;
  });
});
