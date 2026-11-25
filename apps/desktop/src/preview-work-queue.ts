export type PreviewPriority = 0 | 1 | 2 | 3;

export interface PreviewQueueStatus {
  active: boolean;
  queued: number;
  activeLabel?: string;
}

interface Work<T = unknown> {
  key: string;
  priority: PreviewPriority;
  label: string;
  sequence: number;
  run: () => Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
}

export class PreviewWorkQueue {
  private queued = new Map<string, Work>();
  private active?: Work;
  private sequence = 0;
  private listeners = new Set<(status: PreviewQueueStatus) => void>();

  constructor(private readonly maxQueued = 64) {}

  schedule<T>(
    key: string,
    priority: PreviewPriority,
    label: string,
    run: () => Promise<T>,
  ): Promise<T> {
    const active = this.active;
    if (active?.key === key)
      return new Promise<T>((resolve, reject) => {
        const originalResolve = active.resolve;
        const originalReject = active.reject;
        active.resolve = (value) => {
          originalResolve(value);
          resolve(value as T);
        };
        active.reject = (reason) => {
          originalReject(reason);
          reject(reason);
        };
      });
    const existing = this.queued.get(key);
    if (existing) {
      existing.priority = Math.min(
        existing.priority,
        priority,
      ) as PreviewPriority;
      this.emit();
      return new Promise<T>((resolve, reject) => {
        const originalResolve = existing.resolve;
        const originalReject = existing.reject;
        existing.resolve = (value) => {
          originalResolve(value);
          resolve(value as T);
        };
        existing.reject = (reason) => {
          originalReject(reason);
          reject(reason);
        };
      });
    }
    if (this.queued.size >= this.maxQueued) {
      const worst = [...this.queued.values()].sort(
        (left, right) =>
          right.priority - left.priority || right.sequence - left.sequence,
      )[0];
      if (!worst || worst.priority <= priority)
        return Promise.reject(new Error('Preview preparation queue is full.'));
      this.queued.delete(worst.key);
      worst.reject(
        new Error(
          'Preview preparation was superseded by higher-priority work.',
        ),
      );
    }
    return new Promise<T>((resolve, reject) => {
      this.queued.set(key, {
        key,
        priority,
        label,
        sequence: this.sequence++,
        run,
        resolve: resolve as (value: unknown) => void,
        reject,
      });
      this.emit();
      this.pump();
    });
  }

  subscribe(listener: (status: PreviewQueueStatus) => void) {
    this.listeners.add(listener);
    listener(this.status);
    return () => {
      this.listeners.delete(listener);
    };
  }

  promote(key: string, priority: PreviewPriority) {
    const work = this.queued.get(key);
    if (!work || work.priority <= priority) return;
    work.priority = priority;
    this.emit();
  }

  get status(): PreviewQueueStatus {
    return {
      active: Boolean(this.active),
      queued: this.queued.size,
      activeLabel: this.active?.label,
    };
  }

  clearQueued() {
    for (const work of this.queued.values())
      work.reject(new Error('Preview preparation queue was cleared.'));
    this.queued.clear();
    this.emit();
  }

  private pump() {
    if (this.active || !this.queued.size) return;
    const next = [...this.queued.values()].sort(
      (left, right) =>
        left.priority - right.priority || left.sequence - right.sequence,
    )[0];
    this.queued.delete(next.key);
    this.active = next;
    this.emit();
    void next
      .run()
      .then(next.resolve, next.reject)
      .finally(() => {
        this.active = undefined;
        this.emit();
        this.pump();
      });
  }

  private emit() {
    const status = this.status;
    for (const listener of this.listeners) listener(status);
  }
}

export const previewWorkQueue = new PreviewWorkQueue();
