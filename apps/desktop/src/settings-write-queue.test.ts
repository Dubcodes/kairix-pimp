import { expect, it, vi } from 'vitest';
import { SettingsWriteQueue } from './settings-write-queue';

interface Preferences {
  theme: string;
  closeToTray: boolean;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

it('serializes rapid writes and commits only the newest native response', async () => {
  const first = deferred<Preferences>();
  const second = deferred<Preferences>();
  const write = vi
    .fn<(value: Preferences) => Promise<Preferences>>()
    .mockReturnValueOnce(first.promise)
    .mockReturnValueOnce(second.promise);
  const onCommitted = vi.fn();
  const pending: boolean[] = [];
  const queue = new SettingsWriteQueue<Preferences>({
    write,
    onCommitted,
    onFailed: vi.fn(),
    onPendingChanged: (value) => pending.push(value),
  });
  const graphite = { theme: 'graphite', closeToTray: true };
  const midnight = { theme: 'midnight', closeToTray: false };

  const firstSave = queue.save(graphite, 'first');
  const secondSave = queue.save(midnight, 'second');
  expect(write).toHaveBeenCalledTimes(0);
  await Promise.resolve();
  expect(write).toHaveBeenCalledWith(graphite);
  expect(write).toHaveBeenCalledTimes(1);
  first.resolve(graphite);
  await firstSave;
  expect(onCommitted).not.toHaveBeenCalled();
  await Promise.resolve();
  expect(write).toHaveBeenCalledWith(midnight);
  second.resolve(midnight);
  await secondSave;

  expect(onCommitted).toHaveBeenCalledTimes(1);
  expect(onCommitted).toHaveBeenCalledWith(midnight, 'second');
  expect(await queue.flush()).toBe(true);
  expect(pending.at(-1)).toBe(false);
});

it('keeps a failed latest snapshot retryable', async () => {
  const write = vi
    .fn<(value: Preferences) => Promise<Preferences>>()
    .mockRejectedValueOnce(new Error('disk busy'))
    .mockImplementationOnce(async (value) => value);
  const onCommitted = vi.fn();
  const onFailed = vi.fn();
  const queue = new SettingsWriteQueue<Preferences>({
    write,
    onCommitted,
    onFailed,
    onPendingChanged: vi.fn(),
  });
  const latest = { theme: 'light', closeToTray: true };

  expect(await queue.save(latest, 'save')).toBe(false);
  expect(queue.hasUnsavedChanges()).toBe(true);
  expect(onFailed).toHaveBeenCalledWith(
    expect.objectContaining({ message: 'disk busy' }),
  );
  expect(await queue.retry('retry')).toBe(true);
  expect(queue.hasUnsavedChanges()).toBe(false);
  expect(onCommitted).toHaveBeenCalledWith(latest, 'retry');
});
