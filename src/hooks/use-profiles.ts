import useSWR, { mutate } from "swr";
import {
  getProfiles,
  patchProfile,
  patchProfilesConfig,
  triggerAutoDelayTest,
  triggerAutoSiteTest,
} from "@/services/cmds";
import { getProxies, updateProxy } from "@/services/api";
import { queryKeys } from "@/shared/query/keys";

export const useProfiles = () => {
  const { data: profiles, mutate: mutateProfiles } = useSWR(
    queryKeys.profiles,
    getProfiles
  );

  const patchProfiles = async (value: Partial<IProfilesConfig>) => {
    await patchProfilesConfig(value);
    mutateProfiles();
  };

  const patchCurrent = async (value: Partial<IProfileItem>) => {
    if (profiles?.current) {
      await patchProfile(profiles.current, value);
      mutateProfiles();
    }
  };

  // 根据selected的节点选择
  const activateSelected = async () => {
    const proxiesData = await getProxies();
    const profileData = await getProfiles();

    if (!profileData || !proxiesData) return;

    const current = profileData.items?.find(
      (e) => e && e.uid === profileData.current
    );

    if (!current) return;

    // init selected array
    const { selected = [] } = current;
    const selectedMap = Object.fromEntries(
      selected.map((each) => [each.name!, each.now!])
    );

    let hasChange = false;

    const newSelected: typeof selected = [];
    const { global, groups } = proxiesData;

    [global, ...groups].forEach(({ type, name, now }) => {
      if (!now || type !== "Selector") return;
      if (selectedMap[name] != null && selectedMap[name] !== now) {
        hasChange = true;
        updateProxy(name, selectedMap[name]);
      }
      newSelected.push({ name, now: selectedMap[name] });
    });

    if (hasChange) {
      patchProfile(profileData.current!, { selected: newSelected });
      mutate(queryKeys.proxies, getProxies());
    }

    // 切换到/新建订阅后补一轮延迟测试和站点检测，让新节点立即有结果
    triggerAutoDelayTest().catch(() => undefined);
    triggerAutoSiteTest().catch(() => undefined);
  };

  return {
    profiles,
    current: profiles?.items?.find((p) => p && p.uid === profiles.current),
    activateSelected,
    patchProfiles,
    patchCurrent,
    mutateProfiles,
  };
};
