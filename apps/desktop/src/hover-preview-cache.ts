import type { HoverPreview } from './bridge';

interface PreviewRequestEntry {
  promise: Promise<HoverPreview>;
  pending: boolean;
  bytes: number;
}

export class HoverPreviewRequestCache {
  private entries = new Map<string, PreviewRequestEntry>();
  private generation = 0;

  private usage = 0;

  constructor(
    private limitBytes = 128 * 1024 * 1024,
    private readonly maxPending = 64,
  ) {}

  configure(limitBytes: number) {
    this.limitBytes = Math.max(1, limitBytes);
    this.evictToBudget();
  }

  request(key: string, loader: () => Promise<HoverPreview>) {
    const existing = this.entries.get(key);
    if (existing) {
      this.entries.delete(key);
      this.entries.set(key, existing);
      return existing.promise;
    }

    const pendingCount = [...this.entries.values()].filter(
      (entry) => entry.pending,
    ).length;
    if (pendingCount >= this.maxPending) {
      return Promise.reject(
        new Error(
          'Hover preview queue is busy; move the pointer and try again.',
        ),
      );
    }

    const generation = this.generation;
    const entry: PreviewRequestEntry = {
      pending: true,
      bytes: 0,
      promise: Promise.resolve(undefined as never),
    };
    entry.promise = loader()
      .then((preview) => {
        if (generation !== this.generation) {
          throw new Error('Hover preview cache was cleared.');
        }
        entry.pending = false;
        entry.bytes = approximatePreviewBytes(preview);
        this.usage += entry.bytes;
        this.evictToBudget(key);
        return preview;
      })
      .catch((error: unknown) => {
        if (this.entries.get(key) === entry) this.deleteEntry(key);
        throw error;
      });
    this.entries.set(key, entry);
    return entry.promise;
  }

  invalidate() {
    this.generation += 1;
    this.entries.clear();
    this.usage = 0;
  }

  private deleteEntry(key: string) {
    const entry = this.entries.get(key);
    if (!entry) return;
    this.usage = Math.max(0, this.usage - entry.bytes);
    this.entries.delete(key);
  }

  private evictToBudget(keep?: string) {
    for (const [key, entry] of this.entries) {
      if (this.usage <= this.limitBytes) break;
      if (key === keep || entry.pending) continue;
      this.deleteEntry(key);
    }
  }

  get size() {
    return this.entries.size;
  }

  get usageBytes() {
    return this.usage;
  }
}

export function approximatePreviewBytes(preview: HoverPreview) {
  return (
    preview.frames.reduce((total, frame) => total + frame.length * 2, 0) +
    preview.frameTimes.length * 8
  );
}

export const hoverPreviewRequestCache = new HoverPreviewRequestCache();
