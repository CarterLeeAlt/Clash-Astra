import { useCallback, useEffect, useMemo, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useRecoilValue, useSetRecoilState } from "recoil";
import { getClashLogs } from "@/services/cmds";
import { getVersion } from "@/services/api";
import { useClashInfo } from "@/hooks/use-clash";
import {
  atomEnableLog,
  atomLogConnState,
  atomLogData,
  atomLogError,
} from "@/services/states";
import { useWebsocket } from "@/hooks/use-websocket";
import { buildControllerWsUrl } from "@/utils/controller";
import { mergeLogBatch, normalizeLogItem } from "@/features/logs/model";

const LOG_FLUSH_INTERVAL = 200;
const FALLBACK_SYNC_INTERVAL = 4000;
const RECONNECTING_HINT_DELAY = 8000;

export const useLogStream = () => {
  const { clashInfo } = useClashInfo();
  const enableLog = useRecoilValue(atomEnableLog);
  const setLogData = useSetRecoilState(atomLogData);
  const setLogError = useSetRecoilState(atomLogError);
  const setLogConnState = useSetRecoilState(atomLogConnState);

  const historyReqIdRef = useRef(0);
  const fallbackTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reconnectHintTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null
  );
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<ILogItem[]>([]);
  const unmountedRef = useRef(false);

  useEffect(() => {
    // StrictMode 的开发模式会 mount -> unmount -> mount，挂载时需复位
    unmountedRef.current = false;
    return () => {
      unmountedRef.current = true;
    };
  }, []);

  const wsUrl = useMemo(
    () =>
      buildControllerWsUrl(
        clashInfo?.server || "",
        "/logs",
        clashInfo?.secret || ""
      ),
    [clashInfo?.server, clashInfo?.secret]
  );

  const clearFallbackTimer = useCallback(() => {
    if (!fallbackTimerRef.current) return;
    clearTimeout(fallbackTimerRef.current);
    fallbackTimerRef.current = null;
  }, []);

  const clearReconnectHintTimer = useCallback(() => {
    if (!reconnectHintTimerRef.current) return;
    clearTimeout(reconnectHintTimerRef.current);
    reconnectHintTimerRef.current = null;
  }, []);

  const enterReconnectingState = useCallback(() => {
    setLogConnState("reconnecting");
    clearReconnectHintTimer();
    reconnectHintTimerRef.current = setTimeout(() => {
      setLogError("日志连接恢复中…");
    }, RECONNECTING_HINT_DELAY);
  }, [clearReconnectHintTimer, setLogConnState, setLogError]);

  const clearFlushTimer = useCallback(() => {
    if (!flushTimerRef.current) return;
    clearTimeout(flushTimerRef.current);
    flushTimerRef.current = null;
  }, []);

  const flushLogs = useCallback(() => {
    flushTimerRef.current = null;
    const batch = pendingRef.current;
    pendingRef.current = [];
    if (batch.length === 0) return;
    setLogData((old) => mergeLogBatch(old, batch));
  }, [setLogData]);

  const clearLogs = useCallback(() => {
    pendingRef.current = [];
    clearFlushTimer();
    setLogData([]);
  }, [clearFlushTimer, setLogData]);

  // 高频日志先入缓冲，200ms 批量合并一次，避免逐条触发整表重渲
  const mergeLogs = useCallback(
    (incoming: ILogItem[]) => {
      if (!incoming || incoming.length === 0) return;
      pendingRef.current.push(...incoming);
      if (!flushTimerRef.current) {
        flushTimerRef.current = setTimeout(flushLogs, LOG_FLUSH_INTERVAL);
      }
    },
    [flushLogs]
  );

  const pullHistory = useCallback(
    async (source: "ws-open" | "fallback" | "manual" = "manual") => {
      const reqId = ++historyReqIdRef.current;
      try {
        const logs = await getClashLogs();
        if (reqId !== historyReqIdRef.current) return;
        // 历史保持「旧 -> 新」入缓冲，由 flushLogs 统一反转一次前置；
        // 若在此处再 reverse，会与 flushLogs 的反转相互抵消，导致历史段倒序
        mergeLogs((logs || []).map(normalizeLogItem));

        if (source === "ws-open") {
          setLogError(null);
          setLogConnState("connected");
          clearReconnectHintTimer();
        }
      } catch (err) {
        if (reqId !== historyReqIdRef.current) return;
        console.warn("[log] history sync failed", err);
      }
    },
    [clearReconnectHintTimer, mergeLogs, setLogConnState, setLogError]
  );

  const { connect, disconnect, isOpen } = useWebsocket(
    (event) => {
      try {
        const data = JSON.parse(event.data) as ILogItem;
        mergeLogs([normalizeLogItem(data)]);
      } catch {
        // ignore invalid event payload
      }
    },
    {
      reconnect: true,
      maxRetries: "infinite",
      retryInterval: 500,
      maxRetryInterval: 10000,
      backoff: true,
      onOpen: () => {
        clearFallbackTimer();
        clearReconnectHintTimer();
        setLogConnState("connected");
        setLogError(null);
        void pullHistory("ws-open");
      },
      onClose: () => {
        enterReconnectingState();
        void pullHistory("fallback");
        scheduleFallbackSync();
      },
      onError: () => {
        enterReconnectingState();
        void pullHistory("fallback");
        scheduleFallbackSync();
      },
    }
  );

  const scheduleFallbackSync = useCallback(() => {
    if (!enableLog) return;
    if (fallbackTimerRef.current) return;

    fallbackTimerRef.current = setTimeout(async () => {
      fallbackTimerRef.current = null;
      await pullHistory("fallback");
      // 探活：controller 不可达而 WS 仍处于 OPEN，说明是半开死链（本地回环下内核卡死），强制重建
      try {
        await getVersion();
      } catch {
        if (wsUrl && isOpen()) {
          enterReconnectingState();
          disconnect();
          connect(wsUrl);
        }
      }
      if (unmountedRef.current) return;
      scheduleFallbackSyncRef.current();
    }, FALLBACK_SYNC_INTERVAL);
  }, [
    connect,
    disconnect,
    enableLog,
    enterReconnectingState,
    isOpen,
    pullHistory,
    wsUrl,
  ]);

  // 递归调度需要引用自身，用 ref 避免自引用导致的闭包过期
  const scheduleFallbackSyncRef = useRef(scheduleFallbackSync);
  scheduleFallbackSyncRef.current = scheduleFallbackSync;

  useEffect(() => {
    let unlisten: null | (() => void) = null;
    listen<ILogItem>("verge://app-log", (event) => {
      mergeLogs([normalizeLogItem(event.payload ?? {})]);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => {
        console.warn("[log] subscribe app log failed", err);
      });

    return () => unlisten?.();
  }, [mergeLogs]);

  useEffect(() => {
    let unlisten: null | (() => void) = null;
    listen("verge://refresh-clash-config", () => {
      if (!enableLog) return;
      enterReconnectingState();
      void pullHistory("fallback");
      if (wsUrl) {
        disconnect();
        connect(wsUrl);
      }
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => {
        console.warn("[log] subscribe refresh-clash-config failed", err);
      });

    return () => unlisten?.();
  }, [
    connect,
    disconnect,
    enableLog,
    enterReconnectingState,
    pullHistory,
    wsUrl,
  ]);

  useEffect(() => {
    if (!enableLog) {
      historyReqIdRef.current += 1;
      clearFallbackTimer();
      clearReconnectHintTimer();
      disconnect();
      setLogConnState("paused");
      setLogError("Log stream paused");
      return;
    }

    if (!wsUrl) {
      historyReqIdRef.current += 1;
      enterReconnectingState();
      disconnect();
      setLogError("Core not running or external-controller is not ready");
      void pullHistory("fallback");
      scheduleFallbackSync();
      return;
    }

    setLogConnState("reconnecting");
    connect(wsUrl);
    void pullHistory("manual");

    return () => {
      historyReqIdRef.current += 1;
      clearFallbackTimer();
      clearReconnectHintTimer();
      clearFlushTimer();
      pendingRef.current = [];
      disconnect();
    };
  }, [
    clearFallbackTimer,
    clearFlushTimer,
    clearReconnectHintTimer,
    connect,
    disconnect,
    enableLog,
    enterReconnectingState,
    pullHistory,
    scheduleFallbackSync,
    setLogConnState,
    setLogError,
    wsUrl,
  ]);

  return { clearLogs };
};
