import dayjs from "dayjs";
import i18next from "i18next";
import relativeTime from "dayjs/plugin/relativeTime";
import { SWRConfig, mutate } from "swr";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Route, Routes, useLocation } from "react-router-dom";
import { CSSTransition, TransitionGroup } from "react-transition-group";
import { alpha, List, Paper, ThemeProvider } from "@mui/material";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { appWindow } from "@tauri-apps/api/window";
import { routers } from "./_routers";
import { getAxios } from "@/services/api";
import { useVerge } from "@/hooks/use-verge";
import LogoSvg from "@/assets/image/logo.svg?react";
import LogoSvg_dark from "@/assets/image/logo_dark.svg?react";
import { atomThemeMode } from "@/services/states";
import { useRecoilState } from "recoil";
import {
  BaseErrorBoundary,
  formatNoticeMessage,
  Notice,
  NoticeManager,
} from "@/components/base";
import { LayoutItem } from "@/components/layout/layout-item";
import { LayoutControl } from "@/components/layout/layout-control";
import { LayoutTraffic } from "@/components/layout/layout-traffic";
import { useCustomTheme } from "@/components/layout/use-custom-theme";
import { useLogSetup } from "@/components/layout/use-log-setup";
import getSystem from "@/utils/get-system";
import "dayjs/locale/ru";
import "dayjs/locale/zh-cn";
import {
  frontendHeartbeat,
  getWindowStyleConfig,
  reportFrontendError,
} from "@/services/cmds";
import { useNavigate } from "react-router-dom";
import { queryKeys } from "@/shared/query/keys";
import { invalidateClashRuntime } from "@/shared/query/invalidate";

dayjs.extend(relativeTime);

const OS = getSystem();

const Layout = () => {
  const [mode] = useRecoilState(atomThemeMode);
  const isDark = mode === "light" ? false : true;
  const { t } = useTranslation();
  const { theme } = useCustomTheme();

  const { verge } = useVerge();
  const { language, start_page } = verge || {};
  const [windowStyle, setWindowStyle] = useState<IWindowStyleConfig>(() => ({
    platform: OS,
    nativeDecorations: OS === "windows",
    reliableMode: OS === "windows",
    customFrameless: false,
  }));
  const nativeDecorations = windowStyle.nativeDecorations;
  const navigate = useNavigate();
  // useNavigate 的返回函数在每次路由变化时都会换新引用（其内部 useCallback
  // 依赖 locationPathname），所以不能直接放进 effect 依赖数组——否则每次点击
  // 标签都会重新触发跳转，把用户弹回 start_page。这里用 ref 保存最新引用，
  // 让 effect 只响应 start_page 的变化。
  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;
  const location = useLocation();

  useLogSetup();

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

    // update the verge config
    unlistenTasks.push(
      listen("verge://refresh-verge-config", () =>
        mutate(queryKeys.vergeConfig)
      )
    );

    // 设置提示监听
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

  useEffect(() => {
    if (!language) return;
    dayjs.locale(language === "zh" ? "zh-cn" : language);
    i18next.changeLanguage(language);
  }, [language]);

  // 只在 start_page 变化时跳转；与语言拆开，避免切换语言时被踢回起始页
  useEffect(() => {
    if (start_page) {
      navigateRef.current(start_page);
    }
  }, [start_page]);

  return (
    <SWRConfig value={{ errorRetryCount: 3 }}>
      <ThemeProvider theme={theme}>
        <NoticeManager />
        <Paper
          square
          elevation={0}
          className={`${OS} layout ${
            nativeDecorations
              ? "native-decorated-window"
              : "custom-frameless-window"
          }`}
          onPointerDown={(e: any) => {
            if (!nativeDecorations && e.target?.dataset?.windrag) {
              appWindow.startDragging();
            }
          }}
          onContextMenu={(e) => {
            // only prevent it on Windows
            const validList = ["input", "textarea"];
            const target = e.currentTarget;
            if (
              OS === "windows" &&
              !(
                validList.includes(
                  (e.target as HTMLElement).tagName.toLowerCase()
                ) || (e.target as HTMLElement).isContentEditable
              )
            ) {
              e.preventDefault();
            }
          }}
          sx={[
            ({ palette }) => ({
              bgcolor: palette.background.paper,
            }),
          ]}
        >
          <div className="layout__left" data-windrag>
            <div className="the-logo" data-windrag>
              {!isDark ? <LogoSvg /> : <LogoSvg_dark />}
            </div>

            <List className="the-menu">
              {routers.map((router) => (
                <LayoutItem
                  key={router.label}
                  to={router.link}
                  icon={router.icon}
                >
                  {t(router.label)}
                </LayoutItem>
              ))}
            </List>

            <div className="the-traffic" data-windrag>
              <LayoutTraffic />
            </div>
          </div>

          <div className="layout__right" data-windrag>
            {OS === "windows" && !nativeDecorations && (
              <div className="the-bar">
                <LayoutControl nativeDecorations={nativeDecorations} />
              </div>
            )}

            <TransitionGroup className="the-content">
              <CSSTransition
                key={location.pathname}
                timeout={300}
                classNames="page"
              >
                <Routes>
                  {routers.map(({ label, link, ele: Ele }) => (
                    <Route
                      key={label}
                      path={link}
                      element={
                        <BaseErrorBoundary key={label}>
                          <Ele />
                        </BaseErrorBoundary>
                      }
                    />
                  ))}
                </Routes>
              </CSSTransition>
            </TransitionGroup>
          </div>
        </Paper>
      </ThemeProvider>
    </SWRConfig>
  );
};

export default Layout;
