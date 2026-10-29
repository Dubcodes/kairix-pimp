interface IdleWork<T = unknown> {
  key: string;
  sequence: number;
  run: () => Promise<T>;
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
}

type IdleCapableWindow = Window & {
  requestIdleCallback?: (callback: () => void) => number;
  cancelIdleCallback?: (handle: number) => void;
};

export class IdlePreviewScheduler {
  private queued = new Map<string, IdleWork>();
  private active?: IdleWork;
  private sequence = 0;
  private pendingHandle?: number;
  private pendingKind?: 'idle' | 'timeout';

  constructor(
    private readonly maxQueued = 64,
    private readonly fallbackDelayMillis = 1_000,
  ) {}

  schedule<T>(key: string, run: () => Promise<T>): Promise<T> {
    const existing = this.queued.get(key) ?? this.active;
    if (existing?.key === key) return existing.promise as Promise<T>;
    if (this.queued.size >= this.maxQueued)
      return Promise.reject(new Error('Idle preview queue is full.'));

    let resolve!: (value: T) => void;
    let reject!: (reason: unknown) => void;
    const promise = new Promise<T>((complete, fail) => {
      resolve = complete;
      reject = fail;
    });
    this.queued.set(key, {
      key,
      sequence: this.sequence++,
      run,
      promise,
      resolve: resolve as (value: unknown) => void,
      reject,
    });
    this.requestPump();
    return promise;
  }

  cancel(key: string) {
    const work = this.queued.get(key);
    if (!work) return false;
    this.queued.delete(key);
    work.reject(new Error('Idle preview preparation was cancelled.'));
    if (!this.queued.size) this.cancelPump();
    return true;
  }

  markInteraction() {
    if (this.pendingHandle == null) return;
    this.cancelPump();
    this.requestPump();
  }

  clear() {
    for (const work of this.queued.values())
      work.reject(new Error('Idle preview queue was cleared.'));
    this.queued.clear();
    this.cancelPump();
  }

  get size() {
    return this.queued.size + (this.active ? 1 : 0);
  }

  private requestPump() {
    if (this.active || this.pendingHandle != null || !this.queued.size) return;
    const idleWindow = window as IdleCapableWindow;
    if (idleWindow.requestIdleCallback) {
      this.pendingKind = 'idle';
      this.pendingHandle = idleWindow.requestIdleCallback(() => this.pump());
    } else {
      this.pendingKind = 'timeout';
      this.pendingHandle = window.setTimeout(
        () => this.pump(),
        this.fallbackDelayMillis,
      );
    }
  }

  private cancelPump() {
    if (this.pendingHandle == null) return;
    const idleWindow = window as IdleCapableWindow;
    if (this.pendingKind === 'idle')
      idleWindow.cancelIdleCallback?.(this.pendingHandle);
    else window.clearTimeout(this.pendingHandle);
    this.pendingHandle = undefined;
    this.pendingKind = undefined;
  }

  private pump() {
    this.pendingHandle = undefined;
    this.pendingKind = undefined;
    if (this.active || !this.queued.size) return;
    const next = [...this.queued.values()].sort(
      (left, right) => left.sequence - right.sequence,
    )[0];
    this.queued.delete(next.key);
    this.active = next;
    void next
      .run()
      .then(next.resolve, next.reject)
      .finally(() => {
        this.active = undefined;
        this.requestPump();
      });
  }
}

export const idlePreviewScheduler = new IdlePreviewScheduler();
