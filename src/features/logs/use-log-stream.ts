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
import { FallbackPoller } from "@/shared/async/fallback-poller";
import {
  MAX_LOG_NUM,
  mergeLogBatch,
  normalizeLogItem,
} from "@/features/logs/model";

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
  const fallbackPollerRef = useRef<FallbackPoller | null>(null);
  const fallbackRunRef = useRef<(isCurrent: () => boolean) => Promise<void>>(
    async () => {}
  );
  if (!fallbackPollerRef.current) {
    fallbackPollerRef.current = new FallbackPoller(
      (isCurrent) => fallbackRunRef.current(isCurrent),
      FALLBACK_SYNC_INTERVAL
    );
  }
  const reconnectHintTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null
  );
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<ILogItem[]>([]);
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
    fallbackPollerRef.current?.stop();
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

  // 高频日志先入缓冲，200ms 批量合并一次，避免逐条触发整表重渲。
  // 窗口隐藏时定时器会被节流到秒级甚至分钟级，缓冲只保留最新的 MAX_LOG_NUM 条
  // （合并后也只保留这么多），防止日志洪峰时无界增长。
  const mergeLogs = useCallback(
    (incoming: ILogItem[]) => {
      if (!incoming || incoming.length === 0) return;
      const pending = pendingRef.current.concat(incoming);
      pendingRef.current =
        pending.length > MAX_LOG_NUM ? pending.slice(-MAX_LOG_NUM) : pending;
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
        // 单条无效消息不影响后续日志流。
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

  fallbackRunRef.current = async (isCurrent) => {
    await pullHistory("fallback");
    if (!isCurrent()) return;
    // 探活：controller 不可达而 WS 仍处于 OPEN，说明是半开死链（本地回环下内核卡死），强制重建
    try {
      await getVersion();
    } catch {
      if (isCurrent() && wsUrl && isOpen()) {
        enterReconnectingState();
        disconnect();
        connect(wsUrl);
      }
    }
    // WS 在等待期间已重连成功（onOpen 只能清掉已经为空的定时器），停止轮询
    if (isCurrent() && isOpen()) fallbackPollerRef.current?.stop();
  };

  const scheduleFallbackSync = useCallback(() => {
    if (enableLog) fallbackPollerRef.current?.start();
  }, [enableLog]);

  // listen 是异步注册的：effect 在注册完成前就被清理时，必须在 resolve 后立即取消，
  // 否则旧监听永远留着，还会带着旧闭包（旧 wsUrl）把共享 WS 重连到旧地址。
  useEffect(() => {
    let disposed = false;
    let unlisten: null | (() => void) = null;
    listen<ILogItem>("verge://app-log", (event) => {
      if (disposed) return;
      mergeLogs([normalizeLogItem(event.payload ?? {})]);
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((err) => {
        console.warn("[log] subscribe app log failed", err);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [mergeLogs]);

  useEffect(() => {
    const invalidate = () => {
      historyReqIdRef.current += 1;
      clearFallbackTimer();
      clearFlushTimer();
      pendingRef.current = [];
      disconnect();
      if (enableLog) enterReconnectingState();
    };
    window.addEventListener("astra://runtime-changed", invalidate);
    return () =>
      window.removeEventListener("astra://runtime-changed", invalidate);
  }, [
    clearFallbackTimer,
    clearFlushTimer,
    disconnect,
    enableLog,
    enterReconnectingState,
  ]);

  useEffect(() => {
    if (!enableLog) {
      historyReqIdRef.current += 1;
      clearFallbackTimer();
      clearReconnectHintTimer();
      disconnect();
      setLogConnState("paused");
      setLogError("Log stream paused");
    } else if (!wsUrl) {
      historyReqIdRef.current += 1;
      enterReconnectingState();
      disconnect();
      setLogError("Core not running or external-controller is not ready");
      void pullHistory("fallback");
      scheduleFallbackSync();
    } else {
      setLogConnState("reconnecting");
      connect(wsUrl);
      void pullHistory("manual");
    }

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
    clashInfo?.runtime_revision,
    clashInfo?.generation,
  ]);

  return { clearLogs };
};
