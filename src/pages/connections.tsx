import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLockFn } from "ahooks";
import {
  Alert,
  Box,
  Button,
  IconButton,
  MenuItem,
  Paper,
  Select,
} from "@mui/material";
import { useRecoilState } from "recoil";
import { Virtuoso } from "react-virtuoso";
import { useTranslation } from "react-i18next";
import { TableChartRounded, TableRowsRounded } from "@mui/icons-material";
import { closeAllConnections, getConnections } from "@/services/api";
import { atomConnectionSetting } from "@/services/states";
import { useClashInfo } from "@/hooks/use-clash";
import { BaseEmpty, BasePage } from "@/components/base";
import { useWebsocket } from "@/hooks/use-websocket";
import { ConnectionItem } from "@/components/connection/connection-item";
import { ConnectionTable } from "@/components/connection/connection-table";
import {
  ConnectionDetail,
  ConnectionDetailRef,
} from "@/components/connection/connection-detail";
import {
  connectionOrderOptions,
  initConn,
  mergeConnectionSnapshot,
} from "@/features/connections/model";
import { FilterTextField } from "@/components/form/filter-text-field";

const ConnectionsPage = () => {
  const { t, i18n } = useTranslation();
  const { clashInfo } = useClashInfo();

  const [filterText, setFilterText] = useState("");
  const [curOrderOpt, setOrderOpt] = useState("Active Speed");
  const [connData, setConnData] = useState<IConnections>(initConn);

  const [setting, setSetting] = useRecoilState(atomConnectionSetting);

  const isTableLayout = setting.layout === "table";

  const [filterConn] = useMemo(() => {
    const orderFunc = connectionOrderOptions[curOrderOpt];
    let connections = connData.connections.filter((conn) =>
      (conn.metadata.host || conn.metadata.destinationIP)?.includes(filterText)
    );

    if (orderFunc) connections = orderFunc(connections);
    return [connections];
  }, [connData, filterText, curOrderOpt]);

  const syncConnections = useLockFn(async () => {
    const snapshot = await getConnections();
    const incoming = snapshot?.connections ?? [];
    setConnData((old) => ({
      ...snapshot,
      connections: mergeConnectionSnapshot(old.connections, incoming),
    }));
  });

  // useLockFn 内部只做了 useCallback(fn)，传入的箭头函数每次渲染都是新引用，
  // 因此 syncConnections 的标识每次渲染都会变。用 ref 固定它，避免下游
  // useCallback 级联失效、以及递归调度时捕获到过期闭包。
  const syncConnectionsRef = useRef(syncConnections);
  syncConnectionsRef.current = syncConnections;

  const [wsState, setWsState] = useState<"connected" | "reconnecting">(
    "reconnecting"
  );
  const fallbackTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const unmountedRef = useRef(false);

  useEffect(() => {
    // StrictMode 的开发模式会 mount -> unmount -> mount，挂载时需复位
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

  // 断线期间用 HTTP 拉快照兜底，WS 恢复后停止
  const scheduleFallbackPolling = useCallback(() => {
    if (fallbackTimerRef.current) return;
    fallbackTimerRef.current = setTimeout(async () => {
      fallbackTimerRef.current = null;
      await syncConnectionsRef.current().catch(() => undefined);
      if (unmountedRef.current) return;
      scheduleFallbackPollingRef.current();
    }, 3000);
  }, []);

  // scheduleFallbackPolling 需要递归调度自身，用 ref 打破自引用
  const scheduleFallbackPollingRef = useRef(scheduleFallbackPolling);
  scheduleFallbackPollingRef.current = scheduleFallbackPolling;

  const { connect, disconnect } = useWebsocket(
    (event) => {
      // meta v1.15.0 出现data.connections为null的情况
      const data = JSON.parse(event.data) as IConnections;
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
      heartbeatTimeout: 5000,
      onOpen: () => {
        setWsState("connected");
        clearFallbackTimer();
        void syncConnections().catch(() => undefined);
      },
      onClose: () => {
        setWsState("reconnecting");
        scheduleFallbackPolling();
      },
      onError: () => {
        setWsState("reconnecting");
        scheduleFallbackPolling();
      },
    }
  );

  useEffect(() => {
    if (!clashInfo?.server) return;

    const { server = "", secret = "" } = clashInfo;
    connect(`ws://${server}/connections?token=${encodeURIComponent(secret)}`);

    return () => {
      clearFallbackTimer();
      disconnect();
    };
  }, [
    clashInfo?.server,
    clashInfo?.secret,
    connect,
    disconnect,
    clearFallbackTimer,
  ]);

  useEffect(() => {
    const onRefresh = () => {
      void syncConnectionsRef.current().catch(() => undefined);
    };
    window.addEventListener("verge://connections-refresh", onRefresh);
    return () =>
      window.removeEventListener("verge://connections-refresh", onRefresh);
  }, []);
  const onCloseAll = useLockFn(async () => {
    await closeAllConnections();
    setConnData(initConn);
    await syncConnections().catch(() => undefined);
  });

  const detailRef = useRef<ConnectionDetailRef>(null!);

  return (
    <BasePage
      full
      title={t("Connections")}
      contentStyle={{ height: "100%" }}
      header={
        <Box sx={{ display: "flex", alignItems: "center", gap: 2 }}>
          <IconButton
            color="inherit"
            size="small"
            onClick={() =>
              setSetting((o) =>
                o.layout === "list"
                  ? { ...o, layout: "table" }
                  : { ...o, layout: "list" }
              )
            }
          >
            {isTableLayout ? (
              <TableChartRounded fontSize="inherit" />
            ) : (
              <TableRowsRounded fontSize="inherit" />
            )}
          </IconButton>

          <Button size="small" variant="contained" onClick={onCloseAll}>
            {t("Close All")}
          </Button>
        </Box>
      }
    >
      {wsState === "reconnecting" && (
        <Box sx={{ px: "10px", pb: 0.5 }}>
          <Alert severity="info" variant="outlined" sx={{ py: 0 }}>
            连接流短暂中断，正在自动重连…
          </Alert>
        </Box>
      )}

      <Box
        sx={{
          pt: 1,
          mb: 0.5,
          mx: "10px",
          height: "36px",
          display: "flex",
          alignItems: "center",
          userSelect: "text",
        }}
      >
        {!isTableLayout && (
          <Select
            size="small"
            autoComplete="off"
            value={curOrderOpt}
            onChange={(e) => setOrderOpt(e.target.value)}
            sx={{
              mr: 1,
              width: i18n.language === "en" ? 190 : 120,
              height: 33.375,
              '[role="button"]': { py: 0.65 },
            }}
          >
            {Object.keys(connectionOrderOptions).map((opt) => (
              <MenuItem key={opt} value={opt}>
                <span style={{ fontSize: 14 }}>{t(opt)}</span>
              </MenuItem>
            ))}
          </Select>
        )}

        <FilterTextField value={filterText} onChange={setFilterText} />
      </Box>

      <Box height="calc(100% - 50px)" sx={{ userSelect: "text" }}>
        {filterConn.length === 0 ? (
          <BaseEmpty text="No Connections" />
        ) : isTableLayout ? (
          <ConnectionTable
            connections={filterConn}
            onShowDetail={(detail) => detailRef.current?.open(detail)}
          />
        ) : (
          <Virtuoso
            data={filterConn}
            itemContent={(index, item) => (
              <ConnectionItem
                value={item}
                onShowDetail={() => detailRef.current?.open(item)}
              />
            )}
          />
        )}
      </Box>
      <ConnectionDetail ref={detailRef} />
    </BasePage>
  );
};

export default ConnectionsPage;
