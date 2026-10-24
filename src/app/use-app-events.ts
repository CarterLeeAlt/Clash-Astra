import { useEffect, useState } from "react";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { appWindow } from "@tauri-apps/api/window";
import { mutate } from "swr";
import { getAxios } from "@/services/api";
import {
  frontendHeartbeat,
  getWindowStyleConfig,
  reportFrontendError,
} from "@/services/cmds";
import { formatNoticeMessage, Notice } from "@/components/base";
import { queryKeys } from "@/shared/query/keys";
import { invalidateClashRuntime } from "@/shared/query/invalidate";
import getSystem from "@/utils/get-system";

const OS = getSystem();

/**
 * 应用级事件与窗口样式：全局快捷键、Tauri 事件、心跳、错误上报。
 * 这些逻辑与页面渲染无关，集中在这里避免 Layout 持续膨胀。
 */
export const useAppEvents = () => {
  const [windowStyle, setWindowStyle] = useState<IWindowStyleConfig>(() => ({
    platform: OS,
    nativeDecorations: OS === "windows",
    reliableMode: OS === "windows",
    customFrameless: false,
  }));

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        appWindow.hide().catch(() => undefined);
      }
    };

    window.addEventListener("keydown", onKeyDown);

    const unlistenTasks: Promise<UnlistenFn>[] = [];

    unlistenTasks.push(
      listen("verge://refresh-clash-config", async () => {
        // the clash info may be updated
        await getAxios(true);
        invalidateClashRuntime();
      })
    );

    unlistenTasks.push(
      listen("verge://refresh-verge-config", () =>
        mutate(queryKeys.vergeConfig)
      )
    );

    unlistenTasks.push(
      listen("verge://notice-message", ({ payload }) => {
        const [status, msg] = payload as [string, string];
        switch (status) {
          case "set_config::ok":
            Notice.success("Clash config refreshed.");
            break;
          case "set_config::error":
            Notice.error(formatNoticeMessage(msg));
            break;
          default:
            break;
        }
      })
    );

    emit("frontend://ready").catch(() => undefined);

    frontendHeartbeat().catch(() => undefined);
    const heartbeatTimer = window.setInterval(() => {
      frontendHeartbeat().catch(() => undefined);
    }, 5000);

    const onError = (event: ErrorEvent) => {
      reportFrontendError(
        event.message || "window.onerror",
        event.error?.stack
      ).catch(() => undefined);
    };
    const onUnhandledRejection = (event: PromiseRejectionEvent) => {
      const reason = event.reason;
      reportFrontendError(
        reason?.message || String(reason || "unhandledrejection"),
        reason?.stack
      ).catch(() => undefined);
    };
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onUnhandledRejection);

    getWindowStyleConfig()
      .then(setWindowStyle)
      .catch(() => undefined);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.clearInterval(heartbeatTimer);
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onUnhandledRejection);
      unlistenTasks.forEach((task) => {
        task.then((unlisten) => unlisten()).catch(() => undefined);
      });
    };
  }, []);

  return windowStyle;
};
