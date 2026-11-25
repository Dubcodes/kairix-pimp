import { describe, expect, it, vi } from 'vitest';
import { PreviewWorkQueue } from './preview-work-queue';

describe('preview work queue', () => {
  it('runs hover work before queued visible, nearby and idle preparation', async () => {
    const queue = new PreviewWorkQueue();
    const order: string[] = [];
    let release!: () => void;
    const active = queue.schedule(
      'active',
      2,
      'nearby',
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    const idle = queue.schedule('idle', 3, 'idle', async () => {
      order.push('idle');
    });
    const visible = queue.schedule('visible', 1, 'visible', async () => {
      order.push('visible');
    });
    const hover = queue.schedule('hover', 0, 'hover', async () => {
      order.push('hover');
    });
    release();
    await Promise.all([active, idle, visible, hover]);
    expect(order).toEqual(['hover', 'visible', 'idle']);
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
});
