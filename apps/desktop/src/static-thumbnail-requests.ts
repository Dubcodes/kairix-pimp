export type StaticThumbnailFailureKind =
  'cancelled' | 'temporary' | 'persistent';

interface FailureState {
  attempts: number;
  retryAt: number;
  kind: StaticThumbnailFailureKind;
}

export class StaticThumbnailBackoffError extends Error {
  constructor(public readonly retryAt: number) {
    super('Static thumbnail retry is temporarily delayed.');
  }
}

const failureKind = (reason: unknown): StaticThumbnailFailureKind => {
  const message = String(reason).toLocaleLowerCase();
  if (
    message.includes('active viewport') ||
    message.includes('superseded') ||
    message.includes('queue was cleared')
  )
    return 'cancelled';
  if (
    message.includes('unknown media') ||
    message.includes('not safe for the thumbnail cache') ||
    message.includes('supported video')
  )
    return 'persistent';
  return 'temporary';
};

export class StaticThumbnailRequestRegistry {
  private readonly inFlight = new Map<string, Promise<string>>();
  private readonly failures = new Map<string, FailureState>();

  constructor(
    private readonly maxFailures = 512,
    private readonly now: () => number = Date.now,
  ) {}

  request(key: string, run: () => Promise<string | undefined>) {
    const existing = this.inFlight.get(key);
    if (existing) return existing;
    const failure = this.failures.get(key);
    if (failure && failure.retryAt > this.now())
      return Promise.reject(new StaticThumbnailBackoffError(failure.retryAt));

    const request = run()
      .then((source) => {
        if (!source)
          throw new Error('Thumbnail generation returned no source.');
        this.failures.delete(key);
        return source;
      })
      .catch((reason) => {
        const kind = failureKind(reason);
        if (kind === 'cancelled') {
          this.failures.delete(key);
        } else {
          const attempts = (this.failures.get(key)?.attempts ?? 0) + 1;
          const base = kind === 'persistent' ? 10_000 : 500;
          const maximum = kind === 'persistent' ? 300_000 : 30_000;
          this.failures.set(key, {
            attempts,
            kind,
            retryAt: this.now() + Math.min(maximum, base * 2 ** (attempts - 1)),
          });
          this.trimFailures();
        }
        throw reason;
      })
      .finally(() => {
        if (this.inFlight.get(key) === request) this.inFlight.delete(key);
      });
    this.inFlight.set(key, request);
    return request;
  }

  reset(key: string) {
    this.failures.delete(key);
  }

  get size() {
    return this.inFlight.size;
  }

  private trimFailures() {
    while (this.failures.size > this.maxFailures) {
      const oldest = this.failures.keys().next().value;
      if (oldest) this.failures.delete(oldest);
      else break;
    }
  }
}
