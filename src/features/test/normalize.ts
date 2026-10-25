export interface NormalizedTestList {
  normalized: IVergeTestItem[];
  changed: boolean;
}

// builtIn 只在前端运行时生效，后端结构体不保存该字段，
// 因此不能把它计入「是否需要回写」，否则每次读取都会判定变化并反复写配置。
const sameItem = (a: IVergeTestItem, b: IVergeTestItem) =>
  a.uid === b.uid && a.name === b.name && a.url === b.url && a.icon === b.icon;

/**
 * 把持久化的测试项与内置项对齐：
 * - 按 uid 优先匹配内置项（允许内置地址升级，不产生重复卡片）
 * - 补齐 builtIn 标记与内置 icon
 * - 缺少内置项时按默认顺序重建，自定义项保持原顺序追加
 */
export const normalizeTestList = (
  list: IVergeTestItem[],
  defaultMap: Map<string, IVergeTestItem>
): NormalizedTestList => {
  const defaults = Array.from(defaultMap.values());
  const byUid = new Map(
    defaults
      .filter((item) => item.uid)
      .map((item) => [item.uid as string, item])
  );

  let changed = false;
  const usedUids = new Set<string>();

  const normalized: IVergeTestItem[] = [];
  for (const item of list) {
    const builtin =
      (item.uid && byUid.get(item.uid)) ||
      defaultMap.get(`${item.name}::${item.url}`);

    if (!builtin || !builtin.uid) {
      normalized.push(item);
      continue;
    }

    if (usedUids.has(builtin.uid)) {
      changed = true;
      continue;
    }
    usedUids.add(builtin.uid);

    const next: IVergeTestItem = {
      ...item,
      uid: builtin.uid,
      name: builtin.name,
      url: builtin.url,
      icon: builtin.icon,
      builtIn: true,
    };
    if (!sameItem(item, next)) changed = true;
    normalized.push(next);
  }

  const missing = defaults.filter(
    (item) => item.uid && !usedUids.has(item.uid)
  );
  if (missing.length === 0) {
    return { normalized, changed };
  }

  const present = new Map(
    normalized
      .filter((item) => item.uid && byUid.has(item.uid))
      .map((item) => [item.uid as string, item])
  );
  const custom = normalized.filter((item) => !item.uid || !byUid.has(item.uid));

  return {
    normalized: [
      ...defaults.map((item) => present.get(item.uid as string) ?? item),
      ...custom,
    ],
    changed: true,
  };
};
