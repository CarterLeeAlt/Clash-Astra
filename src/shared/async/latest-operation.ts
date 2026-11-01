/** One running mutation and one latest intent. Superseded queued clicks never
 * reach IPC; queue time consumes the same seven-second user-operation budget. */
export class LatestOperation<T> {
  private pending?: {
    value: T;
    until: number;
    deadlineUnixMs: number;
    resolve: () => void;
    reject: (error: unknown) => void;
  };
  private running = false;
  private readonly apply: (
    value: T,
    remainingMs: number,
    deadlineUnixMs: number
  ) => Promise<void>;
  private readonly busy: (value: T | undefined) => void;

  constructor(
    apply: (
      value: T,
      remainingMs: number,
      deadlineUnixMs: number
    ) => Promise<void>,
    busy: (value: T | undefined) => void
  ) {
    this.apply = apply;
    this.busy = busy;
  }

  submit(value: T): Promise<void> {
    this.pending?.resolve();
    const result = new Promise<void>((resolve, reject) => {
      this.pending = {
        value,
        until: performance.now() + 7000,
        deadlineUnixMs: Date.now() + 7000,
        resolve,
        reject,
      };
    });
    this.busy(value);
    if (!this.running) void this.drain();
    return result;
  }

  private async drain() {
    this.running = true;
    while (this.pending) {
      const request = this.pending;
      this.pending = undefined;
      try {
        const remaining = Math.floor(request.until - performance.now());
        if (remaining <= 0)
          throw new Error("Switch exceeded the shared 7-second deadline");
        await this.apply(request.value, remaining, request.deadlineUnixMs);
        request.resolve();
      } catch (error) {
        request.reject(error);
      }
    }
    this.running = false;
    this.busy(undefined);
  }
}
