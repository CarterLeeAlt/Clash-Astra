export const GLOBAL_SCRIPT_UID = "__global_script__";

export interface ClassifiedProfiles {
  regularItems: IProfileItem[];
  globalScriptItem?: IProfileItem;
  enhanceItems: IProfileItem[];
}

/** 按 profile 类型分组，并按 chain 顺序排列 merge/script 项。 */
export const classifyProfiles = (
  items: IProfileItem[],
  chain: string[]
): ClassifiedProfiles => {
  const regularTypes = ["local", "remote"];
  const enhanceTypes = ["merge", "script"];
  const globalScriptItem = items.find((item) => item.uid === GLOBAL_SCRIPT_UID);
  const regularItems = items.filter(
    (item) => item && regularTypes.includes(item.type!)
  );
  const restItems = items.filter(
    (item) =>
      item &&
      enhanceTypes.includes(item.type!) &&
      item.uid !== GLOBAL_SCRIPT_UID
  );
  const restMap = Object.fromEntries(restItems.map((item) => [item.uid, item]));
  const enhanceItems = chain
    .map((uid) => restMap[uid])
    .filter(Boolean)
    .concat(restItems.filter((item) => !chain.includes(item.uid)));

  return { regularItems, globalScriptItem, enhanceItems };
};
