import { useEffect } from "react";
import { mutate } from "swr";
import { getProxies } from "@/services/api";
import delayManager from "@/services/delay";
import { queryKeys } from "@/shared/query/keys";

/**
 * 窗口重新可见时清空本地延迟缓存并刷新代理数据。
 * 自动延迟测试在后端直接驱动 mihomo（结果写入内核 delay history），
 * 本地缓存只反映手动测试；窗口隐藏期间的历史更新需要重新可见时让位展示。
 */
export const useDelayRefresh = () => {
  useEffect(() => {
    const onVisibilityChange = () => {
      if (document.visibilityState !== "visible") return;
      delayManager.clearAll();
      mutate(queryKeys.proxies, getProxies());
    };

    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, []);
};
