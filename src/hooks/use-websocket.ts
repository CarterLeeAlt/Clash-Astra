import { useCallback, useEffect, useRef } from "react";

export type WsMsgFn = (event: MessageEvent<any>) => void;

export interface WsOptions {
  errorCount?: number; // legacy alias of maxRetries
  retryInterval?: number; // default is 2500
  maxRetryInterval?: number;
  maxRetries?: number | "infinite";
  reconnect?: boolean;
  backoff?: boolean;
  // 超过该毫秒数未收到任何消息则强制断开并重连（仅适用于服务端有固定推送周期的连接，如 /connections 每秒快照）
  heartbeatTimeout?: number;
  onError?: (event: Event) => void;
  onOpen?: (event: Event) => void;
  onClose?: (event: CloseEvent) => void;
  shouldReconnect?: boolean; // legacy alias of reconnect
}

export const useWebsocket = (onMessage: WsMsgFn, options?: WsOptions) => {
  const wsRef = useRef<WebSocket | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const retryCountRef = useRef(0);
  const manualCloseRef = useRef(false);
  const unmountedRef = useRef(false);
  const urlRef = useRef("");
  const msgRef = useRef(onMessage);
  const optsRef = useRef(options);
  const connIdRef = useRef(0);
  const lastMsgAtRef = useRef(0);

  msgRef.current = onMessage;
  optsRef.current = options;

  const clearTimer = useCallback(() => {
    if (!timerRef.current) return;
    clearTimeout(timerRef.current);
    timerRef.current = null;
  }, []);

  const closeWs = useCallback((ws?: WebSocket | null) => {
    const target = ws ?? wsRef.current;
    if (!target) return;

    target.onopen = null;
    target.onmessage = null;
    target.onerror = null;
    target.onclose = null;

    try {
      target.close();
    } catch {
      // ignore close failures
    }

    if (wsRef.current === target) wsRef.current = null;
  }, []);

  const connect = useCallback((url: string, resetRetry = true) => {
    if (!url || unmountedRef.current) return;

    const prevUrl = urlRef.current;
    manualCloseRef.current = false;
    urlRef.current = url;

    if (resetRetry || prevUrl !== url) retryCountRef.current = 0;

    clearTimer();
    closeWs(wsRef.current);

    const ws = new WebSocket(url);
    wsRef.current = ws;
    const connId = connIdRef.current + 1;
    connIdRef.current = connId;
    lastMsgAtRef.current = Date.now();

    const scheduleReconnect = () => {
      const opts = optsRef.current;
      const reconnectEnabled = (opts?.reconnect ?? opts?.shouldReconnect ?? true) !== false;
      if (!reconnectEnabled || manualCloseRef.current || unmountedRef.current) return;
      if (connIdRef.current !== connId || timerRef.current || !urlRef.current) return;

      const maxRetries = opts?.maxRetries ?? opts?.errorCount ?? 5;
      const normalizedMaxRetries =
        maxRetries === "infinite"
          ? "infinite"
          : Number.isFinite(maxRetries)
            ? Math.max(0, Math.floor(maxRetries))
            : 5;
      if (normalizedMaxRetries !== "infinite" && retryCountRef.current >= normalizedMaxRetries) return;

      const retryInterval = Number.isFinite(opts?.retryInterval)
        ? Math.max(0, opts!.retryInterval as number)
        : 2500;
      const maxRetryInterval = Number.isFinite(opts?.maxRetryInterval)
        ? Math.max(retryInterval, opts!.maxRetryInterval as number)
        : retryInterval;
      const backoff = opts?.backoff ?? false;
      const delay = backoff
        ? Math.min(retryInterval * 2 ** retryCountRef.current, maxRetryInterval)
        : retryInterval;

      retryCountRef.current += 1;
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        if (manualCloseRef.current || unmountedRef.current || connIdRef.current !== connId) return;
        connect(urlRef.current, false);
      }, delay);
    };

    ws.onopen = (event) => {
      if (wsRef.current !== ws || connIdRef.current !== connId) return;
      retryCountRef.current = 0;
      lastMsgAtRef.current = Date.now();
      clearTimer();
      optsRef.current?.onOpen?.(event);
    };

    ws.onmessage = (event) => {
      if (wsRef.current !== ws || connIdRef.current !== connId) return;
      lastMsgAtRef.current = Date.now();
      msgRef.current(event);
    };

    ws.onerror = (event) => {
      if (connIdRef.current !== connId) return;
      optsRef.current?.onError?.(event);
      scheduleReconnect();
    };

    ws.onclose = (event) => {
      if (connIdRef.current !== connId) return;
      if (wsRef.current === ws) wsRef.current = null;
      optsRef.current?.onClose?.(event);
      scheduleReconnect();
    };
  }, [clearTimer, closeWs]);

  const disconnect = useCallback(() => {
    manualCloseRef.current = true;
    retryCountRef.current = 0;
    connIdRef.current += 1;
    clearTimer();
    closeWs();
  }, [clearTimer, closeWs]);

  useEffect(() => {
    // StrictMode 的开发模式会 mount -> unmount -> mount，
    // 必须在挂载时复位，否则第二次挂载后 connect() 会一直被拒绝
    unmountedRef.current = false;
    return () => {
      unmountedRef.current = true;
      disconnect();
    };
  }, [disconnect]);

  // 消息活性看门狗：服务端有固定推送周期的连接（如 /connections 每秒快照）
  // 超时未收到消息时强制重建，处理 TCP 半开/对端卡死但不触发 onclose 的情况
  useEffect(() => {
    const timer = setInterval(() => {
      const timeout = optsRef.current?.heartbeatTimeout;
      if (!timeout || !wsRef.current || unmountedRef.current) return;
      if (Date.now() - lastMsgAtRef.current < timeout) return;
      connect(urlRef.current, false);
    }, 1000);
    return () => clearInterval(timer);
  }, [connect]);

  const isOpen = useCallback(
    () => wsRef.current?.readyState === WebSocket.OPEN,
    [],
  );

  return { connect, disconnect, isOpen };
};
