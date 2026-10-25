import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLockFn } from "ahooks";
import { getConnections } from "@/services/api";
import { useClashInfo } from "@/hooks/use-clash";
import { useVisibility } from "@/hooks/use-visibility";
import { useWebsocket } from "@/hooks/use-websocket";
import { buildControllerWsUrl } from "@/utils/controller";
import { initConn, mergeConnectionSnapshot } from "./model";

const FALLBACK_POLL_INTERVAL = 3000;

/**
 * 连接流的编排：WS 快照、断线兜底轮询、手动刷新和 StrictMode 复位。
 * 页面只消费 connData / wsState，并可用 syncConnections 主动刷新。
 */
export const useConnectionsStream = () => {
  const { clashInfo } = useClashInfo();
  // 窗口隐藏时断开：大连接数下每秒全量解析、合并、重建表格，后台持续堆积内存压力。
  const pageVisible = useVisibility();
  const pageVisibleRef = useRef(pageVisible);
  pageVisibleRef.current = pageVisible;

  const [connData, setConnData] = useState<IConnections>(initConn);
  const [wsState, setWsState] = useState<"connected" | "reconnecting">(
    "reconnecting"
  );

  const syncConnections = useLockFn(async () => {
    const snapshot = await getConnections();
    const incoming = snapshot?.connections ?? [];
    setConnData((old) => ({
      ...snapshot,
      connections: mergeConnectionSnapshot(old.connections, incoming),
    }));
  });

  // useLockFn 的返回值每次渲染都会变；用 ref 固定，避免递归调度捕获旧闭包。
  const syncConnectionsRef = useRef(syncConnections);
  syncConnectionsRef.current = syncConnections;

  const fallbackTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const unmountedRef = useRef(false);

  useEffect(() => {
    // StrictMode 的开发模式会 mount -> unmount -> mount，挂载时需复位。
    unmountedRef.current = false;
    return () => {
      unmountedRef.current = true;
    };
  }, []);

  const clearFallbackTimer = useCallback(() => {
    if (!fallbackTimerRef.current) return;
    clearTimeout(fallbackTimerRef.current);
    fallbackTimerRef.current = null;
  }, []);

  const scheduleFallbackPolling = useCallback(() => {
    if (fallbackTimerRef.current) return;
    fallbackTimerRef.current = setTimeout(async () => {
      fallbackTimerRef.current = null;
      await syncConnectionsRef.current().catch(() => undefined);
      if (unmountedRef.current || !pageVisibleRef.current) return;
      scheduleFallbackPollingRef.current();
    }, FALLBACK_POLL_INTERVAL);
  }, []);

  const scheduleFallbackPollingRef = useRef(scheduleFallbackPolling);
  scheduleFallbackPollingRef.current = scheduleFallbackPolling;

  const { connect, disconnect } = useWebsocket(
    (event) => {
      let data: IConnections;
      try {
        data = JSON.parse(event.data) as IConnections;
      } catch {
        return;
      }
      // meta v1.15.0 出现 data.connections 为 null 的情况。
      const incoming = data.connections ?? [];
      setConnData((old) => ({
        ...data,
        connections: mergeConnectionSnapshot(old.connections, incoming),
      }));
    },
    {
      maxRetries: "infinite",
      retryInterval: 500,
      maxRetryInterval: 10000,
      backoff: true,
      // 内核每秒推送一次；连接数很大时推送可能变慢，阈值太短会反复重连并全量拉快照。
      heartbeatTimeout: 15000,
      onOpen: () => {
        setWsState("connected");
        clearFallbackTimer();
        void syncConnections().catch(() => undefined);
      },
      onClose: () => {
        setWsState("reconnecting");
        if (pageVisibleRef.current) scheduleFallbackPolling();
      },
      onError: () => {
        setWsState("reconnecting");
        if (pageVisibleRef.current) scheduleFallbackPolling();
      },
    }
  );

  const wsUrl = useMemo(
    () =>
      buildControllerWsUrl(
        clashInfo?.server ?? "",
        "/connections",
        clashInfo?.secret ?? ""
      ),
    [clashInfo?.server, clashInfo?.secret]
  );

  useEffect(() => {
    if (!wsUrl || !pageVisible) return;

    connect(wsUrl);

    return () => {
      clearFallbackTimer();
      disconnect();
    };
  }, [wsUrl, pageVisible, connect, disconnect, clearFallbackTimer]);

  useEffect(() => {
    const onRefresh = () => {
      void syncConnectionsRef.current().catch(() => undefined);
    };
    window.addEventListener("verge://connections-refresh", onRefresh);
    return () =>
      window.removeEventListener("verge://connections-refresh", onRefresh);
  }, []);

  const resetConnections = useCallback(() => setConnData(initConn), []);

  return {
    connData,
    wsState,
    syncConnections,
    resetConnections,
  };
};
