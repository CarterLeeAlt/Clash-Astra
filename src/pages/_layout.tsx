import dayjs from "dayjs";
import i18next from "i18next";
import relativeTime from "dayjs/plugin/relativeTime";
import { SWRConfig } from "swr";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Route, Routes, useLocation } from "react-router-dom";
import { CSSTransition, TransitionGroup } from "react-transition-group";
import { alpha, List, Paper, ThemeProvider } from "@mui/material";
import { appWindow } from "@tauri-apps/api/window";
import { routers } from "./_routers";
import { useVerge } from "@/hooks/use-verge";
import brandIcon from "@/assets/image/brand-icon.png";
import { metadata } from "@root/package.json";
import { BaseErrorBoundary, NoticeManager } from "@/components/base";
import { LayoutItem } from "@/components/layout/layout-item";
import { LayoutControl } from "@/components/layout/layout-control";
import { LayoutTraffic } from "@/components/layout/layout-traffic";
import { useCustomTheme } from "@/components/layout/use-custom-theme";
import { LogStreamProvider } from "@/features/logs/log-stream-provider";
import { useAppEvents } from "@/app/use-app-events";
import { useDelayRefresh } from "@/features/proxies/use-delay-refresh";
import getSystem from "@/utils/get-system";
import "dayjs/locale/ru";
import "dayjs/locale/zh-cn";
import { useNavigate } from "react-router-dom";

dayjs.extend(relativeTime);

const OS = getSystem();

const Layout = () => {
  const { t } = useTranslation();
  const { theme } = useCustomTheme();

  const { verge } = useVerge();
  const { language, start_page } = verge || {};
  const windowStyle = useAppEvents();
  useDelayRefresh();
  const nativeDecorations = windowStyle.nativeDecorations;
  const navigate = useNavigate();
  // useNavigate 的返回函数在每次路由变化时都会换新引用（其内部 useCallback
  // 依赖 locationPathname），所以不能直接放进 effect 依赖数组——否则每次点击
  // 标签都会重新触发跳转，把用户弹回 start_page。这里用 ref 保存最新引用，
  // 让 effect 只响应 start_page 的变化。
  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;
  const location = useLocation();

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
    <SWRConfig value={{ errorRetryCount: 3, focusThrottleInterval: 30000 }}>
      <LogStreamProvider>
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
                <img src={brandIcon} alt="" />
                <span className="the-logo-title">{metadata.productName}</span>
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
      </LogStreamProvider>
    </SWRConfig>
  );
};

export default Layout;
