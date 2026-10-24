import dayjs from "dayjs";
import { useMemo, useState } from "react";
import {
  DataGrid,
  GridColDef,
  GridColumnVisibilityModel,
  GridValueFormatterParams,
} from "@mui/x-data-grid";
import { truncateStr } from "@/utils/truncate-str";
import parseTraffic from "@/utils/parse-traffic";

interface Props {
  connections: IConnectionsItem[];
  onShowDetail: (data: IConnectionsItem) => void;
}

const COLUMN_VISIBLE_KEY = "connections-table-columns";

const DEFAULT_COLUMN_VISIBLE: GridColumnVisibilityModel = {
  activeSpeed: false,
  process: true,
};

const loadColumnVisible = (): GridColumnVisibilityModel => {
  try {
    const saved = localStorage.getItem(COLUMN_VISIBLE_KEY);
    if (!saved) return DEFAULT_COLUMN_VISIBLE;
    const parsed = JSON.parse(saved) as GridColumnVisibilityModel;
    // 校验值必须是布尔，防止脏数据进入 DataGrid
    return Object.fromEntries(
      Object.entries(parsed).filter(([, v]) => typeof v === "boolean")
    );
  } catch {
    return DEFAULT_COLUMN_VISIBLE;
  }
};

export const ConnectionTable = (props: Props) => {
  const { connections, onShowDetail } = props;

  const [columnVisible, setColumnVisible] =
    useState<GridColumnVisibilityModel>(loadColumnVisible);

  const onColumnVisibleChange = (model: GridColumnVisibilityModel) => {
    setColumnVisible(model);
    try {
      localStorage.setItem(COLUMN_VISIBLE_KEY, JSON.stringify(model));
    } catch {}
  };

  const columns: GridColDef[] = [
    { field: "host", headerName: "Host", width: 220, minWidth: 220 },
    { field: "process", headerName: "Process", width: 126, minWidth: 84 },
    {
      field: "activeSpeed",
      headerName: "Active Speed",
      width: 130,
      minWidth: 120,
      align: "right",
      headerAlign: "right",
      valueFormatter: (params: GridValueFormatterParams<number>) =>
        parseTraffic(params.value).join(" ") + "/s",
    },
    {
      field: "dlSpeed",
      headerName: "DL Speed",
      width: 99,
      minWidth: 90,
      align: "right",
      headerAlign: "right",
      valueFormatter: (params: GridValueFormatterParams<number>) =>
        parseTraffic(params.value).join(" ") + "/s",
    },
    {
      field: "ulSpeed",
      headerName: "UL Speed",
      width: 99,
      minWidth: 90,
      align: "right",
      headerAlign: "right",
      valueFormatter: (params: GridValueFormatterParams<number>) =>
        parseTraffic(params.value).join(" ") + "/s",
    },

    { field: "chains", headerName: "Chains", width: 192, minWidth: 192 },
    { field: "rule", headerName: "Rule", width: 210, minWidth: 210 },
    {
      field: "time",
      headerName: "Time",
      width: 100,
      minWidth: 100,
      align: "right",
      headerAlign: "right",
      sortComparator: (v1, v2) => {
        return new Date(v2).getTime() - new Date(v1).getTime();
      },
      valueFormatter: (params: GridValueFormatterParams<string>) =>
        dayjs(params.value).fromNow(),
    },
    { field: "source", headerName: "Source", width: 180, minWidth: 180 },
    {
      field: "destinationIP",
      headerName: "Dst IP",
      width: 120,
      minWidth: 120,
    },
    { field: "type", headerName: "Type", width: 160, minWidth: 160 },
  ];

  const connRows = useMemo(() => {
    return connections.map((each) => {
      const { metadata, rulePayload } = each;
      const chains = [...each.chains].reverse().join(" / ");
      const rule = rulePayload ? `${each.rule}(${rulePayload})` : each.rule;
      return {
        id: each.id,
        host: metadata.host
          ? `${metadata.host}:${metadata.destinationPort}`
          : `${metadata.destinationIP}:${metadata.destinationPort}`,
        download: each.download,
        upload: each.upload,
        dlSpeed: each.curDownload,
        ulSpeed: each.curUpload,
        activeSpeed: (each.curDownload ?? 0) + (each.curUpload ?? 0),
        chains,
        rule,
        process: truncateStr(metadata.process || metadata.processPath),
        time: each.start,
        source: `${metadata.sourceIP}:${metadata.sourcePort}`,
        destinationIP: metadata.destinationIP,
        type: `${metadata.type}(${metadata.network})`,

        connectionData: each,
      };
    });
  }, [connections]);

  return (
    <DataGrid
      hideFooter
      rows={connRows}
      columns={columns}
      onRowClick={(e) => onShowDetail(e.row.connectionData)}
      density="compact"
      sx={{ border: "none", "div:focus": { outline: "none !important" } }}
      columnVisibilityModel={columnVisible}
      onColumnVisibilityModelChange={onColumnVisibleChange}
      initialState={{
        sorting: {
          sortModel: [{ field: "activeSpeed", sort: "desc" }],
        },
      }}
    />
  );
};
