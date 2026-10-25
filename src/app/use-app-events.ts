import { useEffect, useState } from "react";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { appWindow } from "@tauri-apps/api/window";
import { mutate } from "swr";
import { getAxios } from "@/services/api";
import {
  getAutoSiteTestResults,
  getWindowStyleConfig,
  reportFrontendError,
  type ITestDelayResult,
} from "@/services/cmds";
import { formatNoticeMessage, Notice } from "@/components/base";
import { queryKeys } from "@/shared/query/keys";
import { invalidateClashRuntime } from "@/shared/query/invalidate";
import {
  setSiteTestResults,
  updateSiteTestResult,
} from "@/features/test/site-test-results";
import getSystem from "@/utils/get-system";

const OS = getSystem();

/**
 * 应用级事件与窗口样式：全局快捷键、Tauri 事件、错误上报（心跳在 main.tsx）。
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
    // Esc 收起窗口只在没有别的东西要响应 Esc 时生效：输入法候选、输入框、
    // 对话框、菜单都会用 Esc 取消，不能顺带把整个窗口藏起来。
    // 收起走关闭请求，由后端 CloseRequested 统一隐藏并保存窗口状态。
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented || e.isComposing) return;
      const target = e.target as HTMLElement | null;
      if (
        target?.closest(
          "input, textarea, select, [contenteditable='true'], .monaco-editor"
        )
      ) {
        return;
      }
      if (
        document.querySelector(
          "[role='dialog'], [role='menu'], [role='listbox'], [role='presentation']"
        )
      ) {
        return;
      }
      appWindow.close().catch(() => undefined);
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

    // 站点检测结果放在全局：切页时页面卸载也不会丢，回来直接读缓存。
    // 先注册监听再拉快照，避免两者之间到达的结果被丢掉。
    unlistenTasks.push(
      listen<{ url: string; result: ITestDelayResult }>(
        "verge://auto-site-test-result",
        ({ payload }) => {
          if (payload?.url && payload.result) {
            updateSiteTestResult(payload.url, payload.result);
          }
        }
      )
    );

    unlistenTasks.push(
      listen<Record<string, ITestDelayResult>>(
        "verge://auto-site-test-results",
        ({ payload }) => {
          setSiteTestResults(payload ?? {});
        }
      )
    );

    getAutoSiteTestResults()
      .then((snapshot) => {
        if (snapshot) {
          setSiteTestResults(
            snapshot.results ?? {},
            snapshot.updatedAt || Date.now()
          );
        }
      })
      .catch(() => undefined);

    emit("frontend://ready").catch(() => undefined);

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
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onUnhandledRejection);
      unlistenTasks.forEach((task) => {
        task.then((unlisten) => unlisten()).catch(() => undefined);
      });
    };
  }, []);

  return windowStyle;
};
