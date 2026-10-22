import { cmdGetProxyDelay } from "./cmds";

const hashKey = (name: string, group: string) => `${group ?? ""}::${name}`;

// 所有出口（含 DIRECT/REJECT）统一使用组级 URL，未设置时用默认地址；
// 只有同一地址下测出的延迟才可比较
const DEFAULT_LATENCY_TEST_URL = "http://www.google.com/generate_204";

// 延迟结果的哨兵值：负数不是毫秒数
export const DELAY_UNKNOWN = -1;
export const DELAY_PENDING = -2;
export const DELAY_NOT_APPLICABLE = -3; // REJECT 没有网络路径，不测
export const DELAY_UNREACHABLE = -4; // DIRECT 直连不到目标，属预期结果
const DELAY_FAILED = 1e6;

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

  setListener(name: string, group: string, listener: (time: number) => void) {
    const key = hashKey(name, group);
    const listeners = this.listenerMap.get(key) ?? new Set();
    listeners.add(listener);
    this.listenerMap.set(key, listeners);
  }

  removeListener(
    name: string,
    group: string,
    listener?: (time: number) => void
  ) {
    const key = hashKey(name, group);
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
    const key = hashKey(name, group);
    this.cache.set(key, [Date.now(), delay]);
    this.listenerMap.get(key)?.forEach((listener) => listener(delay));
    if (!silent) this.groupListenerMap.get(group)?.();
  }

  notifyGroup(group: string) {
    this.groupListenerMap.get(group)?.();
  }

  clearDelay(name: string, group: string, silent = false) {
    const key = hashKey(name, group);
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

  getDelay(name: string, group: string) {
    if (!name) return -1;

    const result = this.cache.get(hashKey(name, group));
    if (result && Date.now() - result[0] <= 18e5) {
      return result[1];
    }
    return -1;
  }

  /// 暂时修复provider的节点延迟排序的问题
  getDelayFix(proxy: IProxyItem, group: string) {
    if (proxy.name === "REJECT") return DELAY_NOT_APPLICABLE;

    const cachedDelay = this.getDelay(proxy.name, group);
    if (cachedDelay === DELAY_PENDING) return cachedDelay;
    if (!proxy.provider && cachedDelay !== DELAY_UNKNOWN) {
      return cachedDelay;
    }

    if (proxy.history.length > 0) {
      // 内核历史里 0ms 表示失败；DIRECT 失败按预期不可达处理
      const delay = proxy.history[proxy.history.length - 1].delay;
      if (delay) return delay;
      return proxy.name === "DIRECT" ? DELAY_UNREACHABLE : DELAY_FAILED;
    }
    return DELAY_UNKNOWN;
  }

  private isAbortError(_err: any, signal?: AbortSignal) {
    return signal?.aborted ?? false;
  }

  private requestDelay(
    name: string,
    group: string,
    timeout: number
  ): Promise<number>;

  private requestDelay(
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

    let delay = DELAY_FAILED;

    try {
      if (signal?.aborted) return null;
      const url = this.getUrl(group)?.trim() || DEFAULT_LATENCY_TEST_URL;
      const result = await cmdGetProxyDelay(name, timeout, url);
      if (signal?.aborted) return null;
      delay = result.delay;
    } catch (err) {
      if (this.isAbortError(err, signal)) return null;
      // DIRECT 失败说明目标对直连不可达，不是节点故障
      delay = name === "DIRECT" ? DELAY_UNREACHABLE : DELAY_FAILED;
    }

    return delay;
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
    if (delay === DELAY_NOT_APPLICABLE) return "N/A";
    if (delay === DELAY_UNREACHABLE) return "Blocked";
    if (delay < 0) return ""; // 未测或测速中，不显示
    if (delay > 1e5 || delay >= timeout) return "Failed";
    return `${delay} ms`;
  }

  formatDelayColor(delay: number, timeout = 5000) {
    if (delay === DELAY_NOT_APPLICABLE || delay === DELAY_UNREACHABLE) {
      return "text.primary";
    }
    if (delay < 0) return "text.secondary";
    if (delay > 1e5 || delay >= timeout) return "error.main";
    if (delay > 500) return "error.main";
    if (delay > 200) return "warning.main";
    return "success.main";
  }
}

export default new DelayManager();
