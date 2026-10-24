import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useVerge } from "@/hooks/use-verge";
import { getProxies } from "@/services/api";
import delayManager, { DEFAULT_LATENCY_TEST_URL } from "@/services/delay";

const AUTO_TEST_CONCURRENCY = 8;

/**
 * 接收后端每 2 分钟发出的自动测速事件，并复用 delayManager 更新节点延迟缓存。
 * 已挂载的代理条目会通过现有监听器直接刷新延迟数字。
 */
export const useAutoDelayTest = () => {
  const { verge } = useVerge();
  const runningRef = useRef(false);

  const rawTimeout = Number(verge?.default_latency_timeout);
  const timeout =
    Number.isInteger(rawTimeout) && rawTimeout > 0 ? rawTimeout : 3000;
  const defaultUrl =
    verge?.default_latency_test?.trim() || DEFAULT_LATENCY_TEST_URL;

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    const run = async () => {
      if (runningRef.current || disposed) return;
      runningRef.current = true;

      try {
        const snapshot = await getProxies();
        const groups = snapshot.groups?.length
          ? snapshot.groups
          : snapshot.global?.all?.length
          ? [snapshot.global]
          : [];

        const tasks = groups.flatMap((group) =>
          group.all
            .filter((proxy) => proxy.name !== "REJECT")
            .map((proxy) => ({ group: group.name, name: proxy.name }))
        );

        if (disposed || tasks.length === 0) return;

        for (const group of groups) {
          if (!delayManager.getUrl(group.name)) {
            delayManager.setUrl(group.name, defaultUrl);
          }
        }

        let next = 0;
        const worker = async () => {
          while (!disposed) {
            const task = tasks[next++];
            if (!task) return;
            await delayManager.checkDelay(task.name, task.group, timeout);
          }
        };

        await Promise.all(
          Array.from(
            { length: Math.min(AUTO_TEST_CONCURRENCY, tasks.length) },
            () => worker()
          )
        );
      } catch (err) {
        console.warn("[delay] auto test failed", err);
      } finally {
        runningRef.current = false;
      }
    };

    listen("verge://auto-delay-test", () => {
      void run();
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch((err) => {
        console.warn("[delay] subscribe auto test failed", err);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [defaultUrl, timeout]);
};
