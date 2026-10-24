import { useMemo, useRef, useState } from "react";
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
import { closeAllConnections } from "@/services/api";
import { atomConnectionSetting } from "@/services/states";
import { BaseEmpty, BasePage } from "@/components/base";
import { ConnectionItem } from "./components/connection-item";
import { ConnectionTable } from "./components/connection-table";
import {
  ConnectionDetail,
  ConnectionDetailRef,
} from "./components/connection-detail";
import { connectionOrderOptions } from "./model";
import { FilterTextField } from "@/components/form/filter-text-field";
import { useConnectionsStream } from "./use-connections-stream";

const ConnectionsPage = () => {
  const { t, i18n } = useTranslation();

  const [filterText, setFilterText] = useState("");
  const [curOrderOpt, setOrderOpt] = useState("Active Speed");
  const [setting, setSetting] = useRecoilState(atomConnectionSetting);
  const { connData, wsState, syncConnections, resetConnections } =
    useConnectionsStream();

  const isTableLayout = setting.layout === "table";

  const [filterConn] = useMemo(() => {
    const orderFunc = connectionOrderOptions[curOrderOpt];
    let connections = connData.connections.filter((conn) =>
      (conn.metadata.host || conn.metadata.destinationIP)?.includes(filterText)
    );

    if (orderFunc) connections = orderFunc(connections);
    return [connections];
  }, [connData, filterText, curOrderOpt]);

  const onCloseAll = useLockFn(async () => {
    await closeAllConnections();
    resetConnections();
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
        <Box sx={{ pl: "10px", pr: "18px", pb: 0.5 }}>
          <Alert severity="info" variant="outlined" sx={{ py: 0 }}>
            连接流短暂中断，正在自动重连…
          </Alert>
        </Box>
      )}

      <Box
        sx={{
          pt: 1,
          mb: 0.5,
          ml: "10px",
          mr: "18px",
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
