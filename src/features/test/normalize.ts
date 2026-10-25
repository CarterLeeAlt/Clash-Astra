export interface NormalizedTestList {
  normalized: IVergeTestItem[];
  changed: boolean;
}

/**
 * 把持久化的测试项与内置项对齐：补齐 builtIn 标记、恢复内置 uid，
 * 并避免重复 uid。调用方负责提供内置项映射（name::url -> item）。
 */
export const normalizeTestList = (
  list: IVergeTestItem[],
  defaultMap: Map<string, IVergeTestItem>
): NormalizedTestList => {
  let changed = false;
  const usedBuiltinUids = new Set<string>();

  const normalized = list.map((item, index) => {
    const defaultItem = defaultMap.get(`${item.name}::${item.url}`);

    if (!defaultItem) {
      return item;
    }

    const uid = usedBuiltinUids.has(defaultItem.uid)
      ? item.uid === defaultItem.uid
        ? `${defaultItem.uid}-${index}`
        : item.uid
      : defaultItem.uid;
    usedBuiltinUids.add(defaultItem.uid);

    if (item.builtIn === true && item.uid === uid) {
      return item;
    }

    changed = true;
    return { ...item, uid, builtIn: true };
  });

  const defaults = Array.from(defaultMap.values());
  const present = new Map<string, IVergeTestItem>();
  for (const item of normalized) {
    const key = `${item.name}::${item.url}`;
    if (defaultMap.has(key) && !present.has(key)) {
      present.set(key, item);
    }
  }

  const missing = defaults.filter(
    (item) => !present.has(`${item.name}::${item.url}`)
  );
  if (missing.length === 0) {
    return { normalized, changed };
  }

  // 新增内置项时：内置项按默认顺序重排，自定义项保持原顺序追加
  const rebuilt = defaults.map(
    (item) => present.get(`${item.name}::${item.url}`) ?? item
  );
  const custom = normalized.filter(
    (item) => !defaultMap.has(`${item.name}::${item.url}`)
  );

  return { normalized: [...rebuilt, ...custom], changed: true };
};
