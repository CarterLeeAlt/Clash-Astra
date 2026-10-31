import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLockFn } from "ahooks";
import { getConnections } from "@/services/api";
import { useClashInfo } from "@/hooks/use-clash";
import { useVisibility } from "@/hooks/use-visibility";
import { useWebsocket } from "@/hooks/use-websocket";
import { buildControllerWsUrl } from "@/utils/controller";
import { initConn, mergeConnectionSnapshot } from "./model";
import { FallbackPoller } from "@/shared/async/fallback-poller";
import { RequestEpoch } from "@/shared/async/request-epoch";

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
  const epoch = useRef(new RequestEpoch());
  const [wsState, setWsState] = useState<"connected" | "reconnecting">(
    "reconnecting"
  );

  const syncConnections = useLockFn(async (isCurrent = () => true) => {
    const ticket = epoch.current.next();
    const snapshot = await getConnections();
    if (!isCurrent() || !epoch.current.accepts(ticket)) return;
    const incoming = snapshot?.connections ?? [];
    setConnData((old) => ({
      ...snapshot,
      connections: mergeConnectionSnapshot(old.connections, incoming),
    }));
  });

  // 轮询通过 ref 读取当前回调，避免捕获已过期的页面设置。
  const syncConnectionsRef = useRef(syncConnections);
  syncConnectionsRef.current = syncConnections;

  const pollerRef = useRef<FallbackPoller | null>(null);
  if (!pollerRef.current) {
    pollerRef.current = new FallbackPoller(async (isCurrent) => {
      await syncConnectionsRef.current(isCurrent).catch(() => undefined);
    }, FALLBACK_POLL_INTERVAL);
  }
  const clearFallbackTimer = useCallback(() => {
    pollerRef.current?.stop();
  }, []);

  const scheduleFallbackPolling = useCallback(() => {
    pollerRef.current?.start();
  }, []);

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
      epoch.current.invalidate();
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
        epoch.current.invalidate();
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
      epoch.current.invalidate();
      clearFallbackTimer();
      disconnect();
    };
  }, [
    wsUrl,
    pageVisible,
    clashInfo?.runtime_revision,
    clashInfo?.generation,
    connect,
    disconnect,
    clearFallbackTimer,
  ]);

  useEffect(() => {
    const invalidate = () => {
      epoch.current.invalidate();
      clearFallbackTimer();
      disconnect();
      setConnData(initConn);
      setWsState("reconnecting");
    };
    window.addEventListener("astra://runtime-changed", invalidate);
    return () =>
      window.removeEventListener("astra://runtime-changed", invalidate);
  }, [disconnect, clearFallbackTimer]);

  useEffect(() => {
    const onRefresh = () => {
      void syncConnectionsRef.current().catch(() => undefined);
    };
    window.addEventListener("verge://connections-refresh", onRefresh);
    return () =>
      window.removeEventListener("verge://connections-refresh", onRefresh);
  }, []);

  const resetConnections = useCallback(() => {
    epoch.current.invalidate();
    setConnData(initConn);
  }, []);

  return {
    connData,
    wsState,
    syncConnections,
    resetConnections,
  };
};
