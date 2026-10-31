import { useEffect } from "react";
import { mutate } from "swr";
import { listen } from "@tauri-apps/api/event";
import { getProxies } from "@/services/api";
import { getAutoDelayResults } from "@/services/cmds";
import delayManager from "@/services/delay";
import { queryKeys } from "@/shared/query/keys";
import { RequestEpoch } from "@/shared/async/request-epoch";

/**
 * 手动与自动测速共用同一套延迟状态：
 * 1. 手动测速直接写延迟缓存，所有分组通过同名监听同步；
 * 2. 后端自动测速完成后推送 verge://auto-delay-finished，前端立即拉取快照写入缓存；
 * 3. 窗口重新可见时也做一次同样的刷新。
 */
export const useDelayRefresh = () => {
  useEffect(() => {
    let disposed = false;
    const epoch = new RequestEpoch();

    const refresh = async () => {
      const ticket = epoch.next();
      try {
        const snapshot = await getProxies();
        if (disposed || !epoch.accepts(ticket)) return;

        delayManager.clearAll();

        try {
          const resultSnapshot = await getAutoDelayResults();
          const results = resultSnapshot.results;
          if (!disposed && epoch.accepts(ticket) && results) {
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

        if (!disposed && epoch.accepts(ticket)) {
          mutate(queryKeys.proxies, snapshot, false);
        }
      } catch {
        // 代理数据拉取失败时保留现有显示
      }
    };

    const onVisibilityChange = () => {
      if (document.visibilityState !== "visible" || disposed) return;
      void refresh();
    };

    const unlistenTasks = [
      listen("verge://auto-delay-finished", () => {
        void refresh();
      }),
    ];

    document.addEventListener("visibilitychange", onVisibilityChange);
    const runtimeChanged = () => {
      epoch.invalidate();
      void refresh();
    };
    window.addEventListener("astra://runtime-changed", runtimeChanged);
    return () => {
      disposed = true;
      epoch.invalidate();
      window.removeEventListener("astra://runtime-changed", runtimeChanged);
      document.removeEventListener("visibilitychange", onVisibilityChange);
      unlistenTasks.forEach((task) => {
        task.then((unlisten) => unlisten()).catch(() => undefined);
      });
    };
  }, []);
};
