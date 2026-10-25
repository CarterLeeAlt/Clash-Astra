import { cmdGetProxyDelay } from "./cmds";

// 延迟是节点自身的属性：同一节点在不同分组里必须显示同一个值。
// group 只用于组级通知和测试 URL，不参与缓存键。
const cacheKey = (name: string) => name;

// 所有出口（含 DIRECT/REJECT）统一使用组级 URL，未设置时用默认地址；
// 只有同一地址下测出的延迟才可比较
const DEFAULT_LATENCY_TEST_URL = "http://cp.cloudflare.com/generate_204";
export { DEFAULT_LATENCY_TEST_URL };

// 延迟结果的哨兵值：负数不是毫秒数
export const DELAY_UNKNOWN = -1;
export const DELAY_PENDING = -2;
export const DELAY_NOT_APPLICABLE = -3; // REJECT 没有网络路径，不测
export const DELAY_UNREACHABLE = -4; // DIRECT 直连不到目标，属预期结果
const DELAY_FAILED = 1e6;

/** 延迟解析只需要名称、当前选中项与 history，分组与节点都满足 */
export type DelayProxyRef = {
  name: string;
  now?: string;
  history: IProxyItem["history"];
};

/** 是否已有可用于显示的测速结果（未测和测速中不算） */
export const hasDelayResult = (delay: number) =>
  delay !== DELAY_UNKNOWN && delay !== DELAY_PENDING;

type DelayTaskState = {
  id: number;
  aborted: boolean;
  controller: AbortController;
};

class DelayManager {
  private cache = new Map<string, [number, number]>();
  private urlMap = new Map<string, string>();
  private groupTaskMap = new Map<string, DelayTaskState>();
  private proxies = new Map<string, DelayProxyRef>();
  private taskSeq = 0;

  // 每个item的监听
  private listenerMap = new Map<string, Set<(time: number) => void>>();

  // 每个分组的监听
  private groupListenerMap = new Map<string, () => void>();

  setUrl(group: string, url: string) {
    this.urlMap.set(group, url);
  }

  getUrl(group: string) {
    return this.urlMap.get(group);
  }

  /** 注册当前代理表，用于把分组解析到它当前选中的节点 */
  setProxyRecords(proxies: Map<string, DelayProxyRef>) {
    this.proxies = proxies;
  }

  /** 分组自身没有延迟：沿当前选中项取到最终节点；叶子节点返回自身 */
  getDelaySource(proxy: DelayProxyRef): string {
    let current = proxy;
    for (let depth = 0; depth < 8; depth++) {
      const next = current.now ? this.proxies.get(current.now) : undefined;
      if (!next) return current.now || current.name;
      if (next.name === current.name) return current.name;
      current = next;
    }
    return current.name;
  }

  setListener(name: string, listener: (time: number) => void) {
    const key = cacheKey(name);
    const listeners = this.listenerMap.get(key) ?? new Set();
    listeners.add(listener);
    this.listenerMap.set(key, listeners);
  }

  removeListener(name: string, listener?: (time: number) => void) {
    const key = cacheKey(name);
    if (!listener) {
      this.listenerMap.delete(key);
      return;
    }

    const listeners = this.listenerMap.get(key);
    if (!listeners) return;

    listeners.delete(listener);
    if (!listeners.size) this.listenerMap.delete(key);
  }

  setGroupListener(group: string, listener: () => void) {
    this.groupListenerMap.set(group, listener);
  }

  removeGroupListener(group: string) {
    this.groupListenerMap.delete(group);
  }

  cancelGroupCheck(group: string) {
    const task = this.groupTaskMap.get(group);
    if (task) {
      task.aborted = true;
      task.controller.abort();
    }
  }

  startGroupCheck(group: string) {
    this.cancelGroupCheck(group);
    const task = {
      id: ++this.taskSeq,
      aborted: false,
      controller: new AbortController(),
    };
    this.groupTaskMap.set(group, task);
    return task;
  }

  isCurrentGroupCheck(group: string, task: DelayTaskState) {
    return !task.aborted && this.groupTaskMap.get(group)?.id === task.id;
  }

  setDelay(name: string, group: string, delay: number, silent = false) {
    const key = cacheKey(name);
    this.cache.set(key, [Date.now(), delay]);
    this.listenerMap.get(key)?.forEach((listener) => listener(delay));
    if (!silent) this.groupListenerMap.get(group)?.();
  }

  notifyGroup(group: string) {
    this.groupListenerMap.get(group)?.();
  }

  clearDelay(name: string, group: string, silent = false) {
    const key = cacheKey(name);
    this.cache.delete(key);
    this.listenerMap.get(key)?.forEach((listener) => listener(-1));
    if (!silent) this.groupListenerMap.get(group)?.();
  }

  clearGroupDelay(names: string[], group: string, task: DelayTaskState) {
    const validNames = names.filter(Boolean);

    if (!this.isCurrentGroupCheck(group, task)) return false;

    for (const name of validNames) {
      if (!this.isCurrentGroupCheck(group, task)) return false;
      this.clearDelay(name, group, true);
    }

    this.notifyGroup(group);
    return this.isCurrentGroupCheck(group, task);
  }

  setGroupPending(names: string[], group: string, task: DelayTaskState) {
    const validNames = names.filter(Boolean);

    if (!this.isCurrentGroupCheck(group, task)) return false;

    for (const name of validNames) {
      if (!this.isCurrentGroupCheck(group, task)) return false;
      this.setDelay(name, group, -2, true);
    }

    this.notifyGroup(group);
    return this.isCurrentGroupCheck(group, task);
  }

  getDelay(name: string) {
    if (!name) return -1;

    const result = this.cache.get(cacheKey(name));
    if (result && Date.now() - result[0] <= 18e5) {
      return result[1];
    }
    return -1;
  }

  /** 清空本地延迟缓存（保留监听器），让内核 delay history 的结果重新可见 */
  clearAll() {
    this.cache.clear();
  }

  /// 分组显示其当前选中节点的延迟；叶子节点用自身缓存与 history
  getDelayFix(proxy: IProxyItem) {
    const sourceName = this.getDelaySource(proxy);
    if (sourceName === "REJECT") return DELAY_NOT_APPLICABLE;

    const cachedDelay = this.getDelay(sourceName);
    if (cachedDelay === DELAY_PENDING) return cachedDelay;
    // 本地缓存来自手动两并发低值或后端自动测试快照，优先于乱序的 history
    if (cachedDelay !== DELAY_UNKNOWN) {
      return cachedDelay;
    }

    const source =
      this.proxies.get(sourceName) ??
      (sourceName === proxy.name ? proxy : undefined);
    if (source && source.history.length > 0) {
      // 内核历史里 0ms 表示失败；DIRECT 失败按预期不可达处理
      const delay = source.history[source.history.length - 1].delay;
      if (delay) return delay;
      return source.name === "DIRECT" ? DELAY_UNREACHABLE : DELAY_FAILED;
    }
    return DELAY_UNKNOWN;
  }

  private async requestDelay(
    name: string,
    group: string,
    timeout: number
  ): Promise<number>;

  private async requestDelay(
    name: string,
    group: string,
    timeout: number,
    signal: AbortSignal
  ): Promise<number | null>;

  private async requestDelay(
    name: string,
    group: string,
    timeout: number,
    signal?: AbortSignal
  ) {
    // REJECT 不是网络路径，任何地址都只会换来一次注定失败的请求
    if (name === "REJECT") return DELAY_NOT_APPLICABLE;

    if (signal?.aborted) return null;
    const url = this.getUrl(group)?.trim() || DEFAULT_LATENCY_TEST_URL;

    // 同一节点并发两轮，取较低值以抑制单次抖动
    const settled = await Promise.allSettled([
      cmdGetProxyDelay(name, timeout, url),
      cmdGetProxyDelay(name, timeout, url),
    ]);
    if (signal?.aborted) return null;

    const delays = settled
      .filter(
        (result): result is PromiseFulfilledResult<{ delay: number }> =>
          result.status === "fulfilled"
      )
      .map((result) => result.value.delay)
      .filter((delay) => typeof delay === "number" && !Number.isNaN(delay));

    if (delays.length > 0) return Math.min(...delays);

    // 两轮都失败：DIRECT 说明目标对直连不可达，不是节点故障
    return name === "DIRECT" ? DELAY_UNREACHABLE : DELAY_FAILED;
  }

  async checkDelay(name: string, group: string, timeout: number) {
    const delay = await this.requestDelay(name, group, timeout);

    this.setDelay(name, group, delay);
    return delay;
  }

  async checkListDelay(
    nameList: string[],
    group: string,
    timeout: number,
    concurrency = 36,
    task = this.startGroupCheck(group)
  ) {
    const names = nameList.filter(Boolean);

    // 设置正在延迟测试中，必须在任何 await 前同步覆盖上一轮结果
    if (!this.setGroupPending(names, group, task)) return false;

    let nextIndex = 0;
    const workerCount = Math.min(concurrency, names.length);

    const help = async (): Promise<void> => {
      while (this.isCurrentGroupCheck(group, task)) {
        const name = names[nextIndex++];
        if (!name) return;

        const delay = await this.requestDelay(
          name,
          group,
          timeout,
          task.controller.signal
        );

        if (!this.isCurrentGroupCheck(group, task)) return;
        if (delay == null) return;
        this.setDelay(name, group, delay);
      }
    };

    await Promise.allSettled(Array.from({ length: workerCount }, () => help()));

    return this.isCurrentGroupCheck(group, task);
  }

  formatDelay(delay: number, timeout = 5000) {
    if (delay === DELAY_NOT_APPLICABLE) return "REJECT";
    if (delay === DELAY_UNREACHABLE) return "BLOCK";
    if (delay < 0) return ""; // 未测或测速中，不显示
    if (delay > 1e5 || delay >= timeout) return "FAILED";
    return `${delay} ms`;
  }

  // 颜色只表达“能不能用”：绿、橙、深橙表示可用且由快到慢；
  // 红表示不可用（失败、超时、被阻断），灰表示没有可比较的结果。
  formatDelayColor(delay: number, timeout = 5000) {
    if (delay === DELAY_NOT_APPLICABLE) return "text.secondary";
    if (delay === DELAY_UNREACHABLE) return "error.main";
    if (delay < 0) return "text.secondary";
    if (delay > 1e5 || delay >= timeout) return "error.main";
    if (delay > 200) return "warning.dark";
    if (delay >= 150) return "warning.main";
    return "success.main";
  }
}

export default new DelayManager();
