export interface SettingsWriteCallbacks<T> {
  write: (value: T) => Promise<T>;
  onCommitted: (value: T, message: string) => void;
  onFailed: (error: unknown) => void;
  onPendingChanged: (pending: boolean) => void;
}

/**
 * Serializes device-settings writes while allowing the UI to keep accepting
 * edits. Older native responses never replace a newer local snapshot.
 */
export class SettingsWriteQueue<T> {
  private tail: Promise<void> = Promise.resolve();
  private revision = 0;
  private pending = 0;
  private latest?: T;
  private unsaved = false;

  constructor(private readonly callbacks: SettingsWriteCallbacks<T>) {}

  setLatest(value: T): void {
    this.latest = value;
  }

  save(value: T, message: string): Promise<boolean> {
    this.latest = value;
    this.unsaved = true;
    const revision = ++this.revision;
    this.pending += 1;
    this.callbacks.onPendingChanged(true);

    const operation = this.tail.then(async () => {
      try {
        const saved = await this.callbacks.write(value);
        if (revision === this.revision) {
          this.latest = saved;
          this.unsaved = false;
          this.callbacks.onCommitted(saved, message);
        }
        return true;
      } catch (error) {
        if (revision === this.revision) this.callbacks.onFailed(error);
        return false;
      } finally {
        this.pending -= 1;
        this.callbacks.onPendingChanged(this.pending > 0);
      }
    });
    this.tail = operation.then(() => undefined);
    return operation;
  }

  retry(message: string): Promise<boolean> {
    if (!this.latest) return Promise.resolve(false);
    return this.save(this.latest, message);
  }

  async flush(): Promise<boolean> {
    await this.tail;
    return !this.unsaved;
  }

  hasUnsavedChanges(): boolean {
    return this.unsaved;
  }
}
