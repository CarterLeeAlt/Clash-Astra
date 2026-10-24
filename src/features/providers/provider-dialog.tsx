import dayjs from "dayjs";
import useSWR, { mutate } from "swr";
import { useState } from "react";
import {
  alpha,
  Box,
  Button,
  Divider,
  IconButton,
  keyframes,
  LinearProgress,
  List,
  ListItem,
  ListItemText,
  styled,
  Typography,
} from "@mui/material";
import { RefreshRounded } from "@mui/icons-material";
import { useTranslation } from "react-i18next";
import {
  getProxyProviders,
  getRuleProviders,
  proxyProviderUpdate,
  ruleProviderUpdate,
} from "@/services/api";
import { BaseDialog } from "@/components/base";
import parseTraffic from "@/utils/parse-traffic";
import { queryKeys } from "@/shared/query/keys";

type ProviderKind = "proxy" | "rule";

interface Props {
  kind: ProviderKind;
}

const round = keyframes`
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
`;

const PROVIDER_META = {
  proxy: {
    title: "Proxy Provider",
    dataKey: queryKeys.proxyProviders,
    refreshKey: queryKeys.proxies,
    fetcher: getProxyProviders,
    update: proxyProviderUpdate,
  },
  rule: {
    title: "Rule Provider",
    dataKey: queryKeys.ruleProviders,
    refreshKey: queryKeys.rules,
    fetcher: getRuleProviders,
    update: ruleProviderUpdate,
  },
} as const;

export const ProviderDialog = ({ kind }: Props) => {
  const { t } = useTranslation();
  const meta = PROVIDER_META[kind];
  const { data } = useSWR(meta.dataKey, meta.fetcher as () => Promise<any>);

  const [open, setOpen] = useState(false);
  const [updating, setUpdating] = useState<Record<string, boolean>>({});

  const providers = (data || {}) as Record<string, any>;
  const hasProvider = Object.keys(providers).length > 0;

  const handleUpdate = async (key: string) => {
    setUpdating((prev) => ({ ...prev, [key]: true }));
    try {
      await meta.update(key);
    } finally {
      await mutate(meta.refreshKey);
      await mutate(meta.dataKey);
      setUpdating((prev) => ({ ...prev, [key]: false }));
    }
  };

  if (!hasProvider) return null;

  const updateAll = () =>
    Promise.all(Object.keys(providers).map((key) => handleUpdate(key)));

  return (
    <>
      <Button
        size="small"
        variant="outlined"
        sx={{ textTransform: "capitalize" }}
        onClick={() => setOpen(true)}
      >
        {t("Provider")}
      </Button>

      <BaseDialog
        open={open}
        title={
          <Box display="flex" justifyContent="space-between" gap={1}>
            <Typography variant="h6">{t(meta.title)}</Typography>
            <Button variant="contained" size="small" onClick={updateAll}>
              {t("Update All")}
            </Button>
          </Box>
        }
        contentSx={{ width: 400 }}
        disableOk
        cancelBtn={t("Cancel")}
        onClose={() => setOpen(false)}
        onCancel={() => setOpen(false)}
      >
        <List sx={{ py: 0, minHeight: 250 }}>
          {Object.entries(providers).map(([key, item]) => {
            const time = dayjs(item.updatedAt);
            const isProxy = kind === "proxy";
            const sub = item.subscriptionInfo;
            const hasSubInfo = !!sub;
            const upload = sub?.Upload || 0;
            const download = sub?.Download || 0;
            const total = sub?.Total || 0;
            const expire = sub?.Expire || 0;
            const progress = Math.round(
              ((download + upload) * 100) / (total + 0.1)
            );

            return (
              <ListItem
                key={key}
                sx={{
                  p: 0,
                  borderRadius: "10px",
                  border: "solid 2px var(--divider-color)",
                  mb: 1,
                }}
              >
                <ListItemText
                  sx={{ px: 1 }}
                  primary={
                    <>
                      <Typography
                        variant="h6"
                        component="span"
                        noWrap
                        title={key}
                      >
                        {key}
                      </Typography>
                      <TypeTag component="span" tone="secondary">
                        {isProxy ? item.proxies.length : item.ruleCount}
                      </TypeTag>
                    </>
                  }
                  secondary={
                    <>
                      <TypeTag component="span" tone="primary">
                        {item.vehicleType}
                      </TypeTag>
                      {!isProxy && item.behavior && (
                        <TypeTag component="span" tone="primary">
                          {item.behavior}
                        </TypeTag>
                      )}
                      <TypeTag component="span" tone="primary">
                        {t("Update At")} {time.fromNow()}
                      </TypeTag>
                      {isProxy && hasSubInfo && (
                        <>
                          <Box
                            sx={{
                              height: 26,
                              display: "flex",
                              alignItems: "center",
                              justifyContent: "space-between",
                              fontSize: 14,
                            }}
                          >
                            <span title="Used / Total">
                              {parseTraffic(upload + download)} /{" "}
                              {parseTraffic(total)}
                            </span>
                            <span title="Expire Time">
                              {parseExpire(expire)}
                            </span>
                          </Box>
                          <LinearProgress
                            variant="determinate"
                            value={progress}
                          />
                        </>
                      )}
                    </>
                  }
                />
                <Divider orientation="vertical" flexItem />
                <IconButton
                  size="small"
                  color="inherit"
                  title="Update Provider"
                  onClick={() => handleUpdate(key)}
                  sx={{
                    animation: updating[key]
                      ? `1s linear infinite ${round}`
                      : "none",
                  }}
                >
                  <RefreshRounded />
                </IconButton>
              </ListItem>
            );
          })}
        </List>
      </BaseDialog>
    </>
  );
};

const TypeTag = styled(Box, {
  shouldForwardProp: (prop) => prop !== "tone",
})<{ tone?: "primary" | "secondary" }>(({ theme, tone = "secondary" }) => ({
  display: "inline-block",
  border: "1px solid #ccc",
  borderColor: alpha(theme.palette[tone].main, 0.5),
  color: alpha(theme.palette[tone].main, 0.8),
  borderRadius: 4,
  fontSize: 10,
  marginRight: "4px",
  padding: "0 2px",
  lineHeight: 1.25,
}));

const parseExpire = (expire?: number) => {
  if (!expire) return "-";
  return dayjs(expire * 1000).format("YYYY-MM-DD");
};
