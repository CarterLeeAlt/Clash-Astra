/** 初始化失败不缓存；失效请求只能等待当前代次结果，不能发布旧客户端。 */
export class LatestResource<T> {
  private generation = 0;
  private value: T | undefined;
  private pending: Promise<T> | undefined;
  private load: () => Promise<T>;

  constructor(load: () => Promise<T>) {
    this.load = load;
  }

  invalidate() {
    this.generation += 1;
    this.value = undefined;
    this.pending = undefined;
  }

  get(): Promise<T> {
    if (this.value !== undefined) return Promise.resolve(this.value);
    if (this.pending) return this.pending;
    const generation = this.generation;
    const pending = this.load()
      .then(
        (value) => {
          if (generation !== this.generation) return this.get();
          this.value = value;
          return value;
        },
        (error) => {
          if (generation !== this.generation) return this.get();
          throw error;
        }
      )
      .finally(() => {
        if (this.pending === pending) this.pending = undefined;
      });
    this.pending = pending;
    return pending;
  }
}
