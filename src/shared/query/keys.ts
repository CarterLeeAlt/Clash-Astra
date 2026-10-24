export const queryKeys = {
  clashInfo: "getClashInfo",
  clashConfig: "getClashConfig",
  runtimeConfig: "getRuntimeConfig",
  version: "getVersion",
  proxies: "getProxies",
  proxyProviders: "getProxyProviders",
  rules: "getRules",
  ruleProviders: "getRuleProviders",
  vergeConfig: "getVergeConfig",
  profiles: "getProfiles",
  runtimeLogs: "getRuntimeLogs",
  checkService: "checkService",
} as const;

export type QueryKey = (typeof queryKeys)[keyof typeof queryKeys];
