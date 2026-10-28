import type { HoverPreview } from './bridge';

interface PreviewRequestEntry {
  promise: Promise<HoverPreview>;
  pending: boolean;
}

export class HoverPreviewRequestCache {
  private entries = new Map<string, PreviewRequestEntry>();
  private generation = 0;

  constructor(private readonly limit = 8) {}

  request(key: string, loader: () => Promise<HoverPreview>) {
    const existing = this.entries.get(key);
    if (existing) {
      this.entries.delete(key);
      this.entries.set(key, existing);
      return existing.promise;
    }

    if (this.entries.size >= this.limit) {
      const reusable = [...this.entries].find(([, entry]) => !entry.pending);
      if (!reusable) {
        return Promise.reject(
          new Error(
            'Hover preview queue is busy; move the pointer and try again.',
          ),
        );
      }
      this.entries.delete(reusable[0]);
    }

    const generation = this.generation;
    const entry: PreviewRequestEntry = {
      pending: true,
      promise: Promise.resolve(undefined as never),
    };
    entry.promise = loader()
      .then((preview) => {
        if (generation !== this.generation) {
          throw new Error('Hover preview cache was cleared.');
        }
        entry.pending = false;
        return preview;
      })
      .catch((error: unknown) => {
        if (this.entries.get(key) === entry) this.entries.delete(key);
        throw error;
      });
    this.entries.set(key, entry);
    return entry.promise;
  }

  invalidate() {
    this.generation += 1;
    this.entries.clear();
  }

  get size() {
    return this.entries.size;
  }
}

export const hoverPreviewRequestCache = new HoverPreviewRequestCache();
