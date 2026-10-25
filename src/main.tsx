/// <reference types="vite/client" />
/// <reference types="vite-plugin-svgr/client" />
import "./assets/styles/index.scss";

import { ResizeObserver } from "@juggle/resize-observer";
if (!window.ResizeObserver) {
  window.ResizeObserver = ResizeObserver;
}

import React from "react";
import { createRoot } from "react-dom/client";
import { RecoilRoot } from "recoil";
import { BrowserRouter } from "react-router-dom";
import { BaseErrorBoundary } from "./components/base";
import Layout from "./pages/_layout";
import { frontendHeartbeat } from "./services/cmds";
import "./services/i18n";

// 前端心跳放在 React 之外：看门狗用它判断 JS 事件循环是否还活着。
// 若挂在组件里，顶层错误边界触发、组件卸载后心跳就停了，会被当成卡死结束整个进程；
// 界面崩溃交给顶层边界自动重载处理。
const sendHeartbeat = () => {
  frontendHeartbeat().catch(() => undefined);
};
sendHeartbeat();
window.setInterval(sendHeartbeat, 5000);

const mainElementId = "root";
const container = document.getElementById(mainElementId);

if (!container) {
  throw new Error(
    `No container '${mainElementId}' found to render application`
  );
}

createRoot(container).render(
  <React.StrictMode>
    <RecoilRoot>
      <BaseErrorBoundary autoReload>
        <BrowserRouter>
          <Layout />
        </BrowserRouter>
      </BaseErrorBoundary>
    </RecoilRoot>
  </React.StrictMode>
);
