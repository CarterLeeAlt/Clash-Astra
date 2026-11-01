import { mutate } from "swr";
import { queryKeys } from "./keys";

/** 刷新内核运行时相关缓存，保持调用方原有的刷新范围。 */
export const invalidateClashRuntime = () => {
  mutate(queryKeys.clashInfo);
  mutate(queryKeys.runtimeConfig);
  mutate(queryKeys.checkService);
  mutate(queryKeys.proxies);
  mutate(queryKeys.version);
  mutate(queryKeys.clashConfig);
  mutate(queryKeys.proxyProviders);
  mutate(queryKeys.ruleProviders);
  mutate(queryKeys.rules);
};

export const invalidateProfiles = () => {
  mutate(queryKeys.profiles);
  mutate(queryKeys.runtimeLogs);
};
