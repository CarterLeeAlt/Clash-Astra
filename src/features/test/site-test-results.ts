import type { ITestDelayResult } from "@/services/cmds";

/** 站点结果保留时间：超过后回到初始的 Check 状态。 */
export const SITE_TEST_RESULT_TTL_MS = 2 * 60 * 1000;

export interface SiteTestEntry {
  result: ITestDelayResult;
  at: number;
}

export type SiteTestResults = Record<string, SiteTestEntry>;

let cache: SiteTestResults = {};
const listeners = new Set<(results: SiteTestResults) => void>();

const emit = () => {
  listeners.forEach((listener) => listener(cache));
};

export const getSiteTestResults = () => cache;

/** 只返回未过期的条目，并把过期条目从模块缓存中移除。 */
export const getFreshSiteTestResults = (now = Date.now()) => {
  const next: SiteTestResults = {};
  let changed = false;

  for (const [url, entry] of Object.entries(cache)) {
    if (now - entry.at < SITE_TEST_RESULT_TTL_MS) {
      next[url] = entry;
    } else {
      changed = true;
    }
  }

  if (changed) {
    cache = next;
    emit();
  }
  return cache;
};

export const setSiteTestResults = (
  results: Record<string, ITestDelayResult>,
  at = Date.now()
) => {
  if (at > 0 && Date.now() - at >= SITE_TEST_RESULT_TTL_MS) {
    cache = {};
    emit();
    return;
  }

  const next: SiteTestResults = {};
  for (const [url, result] of Object.entries(results)) {
    next[url] = { result, at };
  }
  cache = next;
  emit();
};

export const updateSiteTestResult = (
  url: string,
  result: ITestDelayResult,
  at = Date.now()
) => {
  cache = { ...cache, [url]: { result, at } };
  emit();
};

export const clearSiteTestResults = () => {
  cache = {};
  emit();
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
