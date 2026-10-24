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

  return { normalized, changed };
};
