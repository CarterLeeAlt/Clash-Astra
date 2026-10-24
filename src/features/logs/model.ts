import dayjs from "dayjs";

export const MAX_LOG_NUM = 2000;

export const getLogKey = (item: Partial<ILogItem>) => {
  const payload =
    item.payload ?? (item as any).message ?? (item as any).msg ?? "";
  return `${item.time ?? ""}|${item.type ?? ""}|${payload}`;
};

export const normalizeLogTime = (time?: string) => {
  if (!time) return dayjs().format("MM-DD HH:mm:ss");
  const parsed = dayjs(time);
  return parsed.isValid() ? parsed.format("MM-DD HH:mm:ss") : time;
};

export const normalizeLogItem = (item: Partial<ILogItem>): ILogItem => ({
  type: item.type ?? "",
  payload: item.payload ?? (item as any).message ?? (item as any).msg ?? "",
  time: normalizeLogTime(item.time),
});

/**
 * 把新一批日志合并到现有列表：保持“新日志在前”，按时间/级别/内容去重，
 * 并限制总条数。调用方传入的 batch 应按到达顺序排列。
 */
export const mergeLogBatch = (old: ILogItem[], batch: ILogItem[]) => {
  if (batch.length === 0) return old;

  const map = new Map<string, ILogItem>();
  [...batch.slice().reverse(), ...old].forEach((item) => {
    map.set(getLogKey(item), item);
  });
  return Array.from(map.values()).slice(0, MAX_LOG_NUM);
};
