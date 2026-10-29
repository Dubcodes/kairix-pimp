// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { IdlePreviewScheduler } from './idle-preview-scheduler';

describe('idle preview scheduler', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('does not enqueue heavy preview work until the browser is idle', async () => {
    let idleCallback!: () => void;
    vi.stubGlobal(
      'requestIdleCallback',
      vi.fn((callback: () => void) => {
        idleCallback = callback;
        return 1;
      }),
    );
    vi.stubGlobal('cancelIdleCallback', vi.fn());
    const scheduler = new IdlePreviewScheduler();
    const run = vi.fn(async () => 'prepared');
    const result = scheduler.schedule('clip-a', run);
    await Promise.resolve();
    expect(run).not.toHaveBeenCalled();
    idleCallback();
    await expect(result).resolves.toBe('prepared');
  });

  it('defers the pending idle callback again when interaction continues', async () => {
    const callbacks: Array<() => void> = [];
    const cancel = vi.fn();
    vi.stubGlobal(
      'requestIdleCallback',
      vi.fn((callback: () => void) => {
        callbacks.push(callback);
        return callbacks.length;
      }),
    );
    vi.stubGlobal('cancelIdleCallback', cancel);
    const scheduler = new IdlePreviewScheduler();
    const run = vi.fn(async () => undefined);
    const result = scheduler.schedule('clip-a', run);
    scheduler.markInteraction();
    expect(cancel).toHaveBeenCalledWith(1);
    expect(run).not.toHaveBeenCalled();
    callbacks[1]();
    await result;
    expect(run).toHaveBeenCalledTimes(1);
  });

  it('deduplicates, bounds and serializes idle work', async () => {
    vi.useFakeTimers();
    const scheduler = new IdlePreviewScheduler(2, 10);
    const order: string[] = [];
    let release!: () => void;
    const first = scheduler.schedule(
      'first',
      () =>
        new Promise<void>((resolve) => {
          order.push('first');
          release = resolve;
        }),
    );
    const duplicate = scheduler.schedule('first', async () => undefined);
    const second = scheduler.schedule('second', async () => {
      order.push('second');
    });
    await expect(
      scheduler.schedule('third', async () => undefined),
    ).rejects.toThrow(/queue is full/);
    await vi.advanceTimersByTimeAsync(10);
    expect(order).toEqual(['first']);
    release();
    await first;
    expect(duplicate).toBe(first);
    await vi.advanceTimersByTimeAsync(10);
    await second;
    expect(order).toEqual(['first', 'second']);
  });
});
