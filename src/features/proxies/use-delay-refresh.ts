import { useEffect } from "react";
import { mutate } from "swr";
import { getProxies } from "@/services/api";
import { getAutoDelayResults } from "@/services/cmds";
import delayManager from "@/services/delay";
import { queryKeys } from "@/shared/query/keys";

/**
 * 窗口重新可见时刷新延迟显示：
 * 1. 清空本地延迟缓存；
 * 2. 拉取后端自动测试快照（每节点两并发取低值）写入本地缓存，优先于乱序的 history；
 * 3. 重新拉取代理数据，让内核 history 与选择状态同步。
 */
export const useDelayRefresh = () => {
  useEffect(() => {
    let disposed = false;

    const refresh = async () => {
      try {
        const snapshot = await getProxies();
        if (disposed) return;

        delayManager.clearAll();

        try {
          const results = await getAutoDelayResults();
          if (!disposed && results) {
            const groups = [snapshot.global, ...snapshot.groups];
            for (const group of groups) {
              for (const proxy of group.all ?? []) {
                const delay = results[proxy.name];
                if (typeof delay === "number") {
                  delayManager.setDelay(proxy.name, group.name, delay, true);
                }
              }
            }
          }
        } catch {
          // 后端快照不可用时退回 history 展示
        }

        if (!disposed) {
          mutate(queryKeys.proxies, getProxies());
        }
      } catch {
        // 代理数据拉取失败时保留现有显示
      }
    };

    const onVisibilityChange = () => {
      if (document.visibilityState !== "visible" || disposed) return;
      void refresh();
    };

    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      disposed = true;
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, []);
};
