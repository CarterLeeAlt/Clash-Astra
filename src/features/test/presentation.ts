// 内置项仅按当前 UID 补充展示图标和只读标记；不改名称、地址、顺序，不回写配置。
export const presentTestList = (
  list: IVergeTestItem[],
  icons: ReadonlyMap<string, Pick<IVergeTestItem, "icon">>
): IVergeTestItem[] =>
  list.map((item) => {
    const builtin = icons.get(item.uid);
    return builtin
      ? { ...item, icon: builtin.icon, builtIn: true }
      : { ...item, builtIn: false };
  });
