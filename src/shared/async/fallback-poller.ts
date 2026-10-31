type Timer = ReturnType<typeof setTimeout>;

/** 停止同时使待执行回调和正在等待的请求失效，阻止旧请求继续调度。 */
export class FallbackPoller {
  private generation = 0;
  private active = false;
  private timer: Timer | undefined;
  private run: (isCurrent: () => boolean) => Promise<void>;
  private interval: number;

  constructor(
    run: (isCurrent: () => boolean) => Promise<void>,
    interval: number
  ) {
    this.run = run;
    this.interval = interval;
  }

  start() {
    if (this.active) return;
    this.active = true;
    this.schedule(++this.generation);
  }

  stop() {
    this.active = false;
    this.generation += 1;
    clearTimeout(this.timer);
    this.timer = undefined;
  }

  private schedule(generation: number) {
    const isCurrent = () => this.active && this.generation === generation;
    if (!isCurrent()) return;
    this.timer = setTimeout(async () => {
      this.timer = undefined;
      if (!isCurrent()) return;
      try {
        await this.run(isCurrent);
      } finally {
        if (isCurrent()) this.schedule(generation);
      }
    }, this.interval);
  }
}
