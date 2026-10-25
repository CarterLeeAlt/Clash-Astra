import type { ITestDelayResult } from "@/services/cmds";

export type SiteTestResults = Record<string, ITestDelayResult>;

let cache: SiteTestResults = {};
const listeners = new Set<(results: SiteTestResults) => void>();

export const getSiteTestResults = () => cache;

export const setSiteTestResults = (results: SiteTestResults) => {
  cache = results;
  listeners.forEach((listener) => listener(cache));
};

/** 页面卸载后结果仍保留在模块缓存里，重新进入测试页不会丢失 */
export const subscribeSiteTestResults = (
  listener: (results: SiteTestResults) => void
) => {
  listeners.add(listener);
  listener(cache);
  return () => {
    listeners.delete(listener);
  };
};
