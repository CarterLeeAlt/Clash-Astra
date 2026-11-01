import { useEffect, useState } from "react";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { appWindow } from "@tauri-apps/api/window";
import { mutate } from "swr";
import {
  getAutoSiteTestResults,
  getWindowStyleConfig,
  reportFrontendError,
  type ITestDelayResult,
} from "@/services/cmds";
import { formatNoticeMessage, Notice } from "@/components/base";
import { queryKeys } from "@/shared/query/keys";
import { invalidateClashRuntime } from "@/shared/query/invalidate";
import { invalidateController } from "@/infrastructure/mihomo/client";
import delayManager from "@/services/delay";
import {
  setSiteTestResults,
  updateSiteTestResult,
  acceptSiteTestVersion,
  setSiteTestRuntime,
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

    let revision = -1;
    unlistenTasks.push(
      listen<{
        revision: number;
        runtimeRevision: number;
        generation: number;
        runtimeChanged: boolean;
        profilesChanged: boolean;
        settingsChanged: boolean;
      }>("astra://configuration-committed", ({ payload }) => {
        if (!payload || payload.revision <= revision) return;
        revision = payload.revision;
        if (payload.settingsChanged) void mutate(queryKeys.clashSettings);
        if (payload.profilesChanged) {
          void mutate(queryKeys.profiles);
          void mutate(queryKeys.proxies);
        }
        if (payload.runtimeChanged) {
          invalidateController();
          delayManager.invalidate();
          setSiteTestRuntime(payload);
          void mutate(queryKeys.runtimeLogs);
          invalidateClashRuntime();
          window.dispatchEvent(new CustomEvent("astra://runtime-changed"));
          window.dispatchEvent(new CustomEvent("clash://connections-refresh"));
        }
      })
    );

    unlistenTasks.push(
      listen<{ runtimeRevision: number; generation: number }>(
        "clash://refresh-clash-config",
        ({ payload }) => {
          // 内核独立重启不改变配置版本，但会使连接与测速缓存失效。
          delayManager.invalidate();
          setSiteTestRuntime(payload);
          invalidateController();
          invalidateClashRuntime();
          window.dispatchEvent(new CustomEvent("astra://runtime-changed"));
        }
      )
    );

    unlistenTasks.push(
      listen("clash://notice-message", ({ payload }) => {
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
      listen<{
        url: string;
        result: ITestDelayResult;
        runId: number;
        sequence: number;
        updatedAt: number;
        runtimeRevision: number;
        generation: number;
      }>("clash://auto-site-test-result", ({ payload }) => {
        if (payload?.url && payload.result && acceptSiteTestVersion(payload)) {
          updateSiteTestResult(payload.url, payload.result, payload.updatedAt);
        }
      })
    );

    unlistenTasks.push(
      listen<import("@/services/cmds").ISiteTestSnapshot>(
        "clash://auto-site-test-results",
        ({ payload }) => {
          if (payload && acceptSiteTestVersion(payload))
            setSiteTestResults(payload.results, payload.updatedAt);
        }
      )
    );

    Promise.all(unlistenTasks)
      .then(() => getAutoSiteTestResults())
      .then((snapshot) => {
        if (snapshot && acceptSiteTestVersion(snapshot)) {
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
