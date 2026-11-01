import useSWR from "swr";
import { useLockFn } from "ahooks";
import { useRef, useState } from "react";
import { LatestOperation } from "@/shared/async/latest-operation";
import { useTranslation } from "react-i18next";
import lockIconUrl from "@/assets/icons/lock.svg";
import lockOpenIconUrl from "@/assets/icons/lock_open.svg";
import restartIconUrl from "@/assets/icons/restart.svg";
import {
  Box,
  Button,
  ButtonGroup,
  CircularProgress,
  IconButton,
  Tooltip,
} from "@mui/material";
import { getClashConfig } from "@/services/api";
import {
  patchClashConfig,
  restartSidecar,
  setWindowSizeLocked,
} from "@/services/cmds";
import { useClashSettings } from "@/hooks/use-clash-settings";
import { BasePage, formatNoticeMessage, Notice } from "@/components/base";
import { ProxyGroups } from "./components/proxy-groups";
import { ProviderButton } from "./components/provider-button";

function LocalSvgIcon(props: { src: string; size?: number }) {
  const { src, size = 22 } = props;

  return (
    <img
      aria-hidden
      src={src}
      draggable={false}
      style={{
        display: "block",
        width: size,
        height: size,
        objectFit: "contain",
        pointerEvents: "none",
        userSelect: "none",
      }}
    />
  );
}

const ProxyPage = () => {
  const { t } = useTranslation();

  const { data: clashConfig, mutate: mutateClash } = useSWR(
    "getClashConfig",
    getClashConfig
  );

  const { clashSettings, mutateClashSettings } = useClashSettings();

  const isSizeLocked = clashSettings?.window_size_locked ?? false;

  const modeList = ["rule", "global", "direct"];

  const curMode = clashConfig?.mode?.toLowerCase();

  const [switchingMode, setSwitchingMode] = useState<string>();
  const modeQueue = useRef<LatestOperation<string>>();
  if (!modeQueue.current)
    modeQueue.current = new LatestOperation<string>(
      async (mode, _remaining, deadlineUnixMs) => {
        try {
          await patchClashConfig({ mode }, deadlineUnixMs);
        } finally {
          void mutateClash();
        }
      },
      setSwitchingMode
    );
  const onChangeMode = (mode: string) =>
    void modeQueue
      .current!.submit(mode)
      .catch((error) => Notice.error(formatNoticeMessage(error)));

  const onRestartCore = useLockFn(async () => {
    try {
      await restartSidecar();
      Notice.success(`Mihomo core restarted.`);
    } catch (err: any) {
      Notice.error(formatNoticeMessage(err));
    }
  });

  const onToggleSizeLocked = useLockFn(async () => {
    try {
      await setWindowSizeLocked(!isSizeLocked);
      await mutateClashSettings();
    } catch (err: any) {
      Notice.error(formatNoticeMessage(err));
    }
  });

  return (
    <BasePage
      full
      contentStyle={{ height: "100%" }}
      title={t("Proxy Groups")}
      header={
        <Box display="flex" alignItems="center" gap={1}>
          <ProviderButton />

          <Box display="flex" alignItems="center">
            <Tooltip
              title={
                isSizeLocked ? t("Unlock Window Size") : t("Lock Window Size")
              }
            >
              <IconButton
                size="small"
                color="inherit"
                onClick={onToggleSizeLocked}
                sx={{ mr: 1 }}
              >
                <LocalSvgIcon
                  src={isSizeLocked ? lockIconUrl : lockOpenIconUrl}
                />
              </IconButton>
            </Tooltip>

            <Tooltip title={t("Restart")}>
              <IconButton
                size="small"
                color="inherit"
                onClick={onRestartCore}
                sx={{ mr: 2.1 }}
              >
                <LocalSvgIcon src={restartIconUrl} />
              </IconButton>
            </Tooltip>

            <ButtonGroup size="small">
              {modeList.map((mode) => (
                <Button
                  aria-busy={switchingMode === mode || undefined}
                  startIcon={
                    switchingMode === mode ? (
                      <CircularProgress size={12} color="inherit" />
                    ) : undefined
                  }
                  key={mode}
                  variant={mode === curMode ? "contained" : "outlined"}
                  onClick={() => onChangeMode(mode)}
                  sx={{ textTransform: "capitalize" }}
                >
                  {t(mode)}
                </Button>
              ))}
            </ButtonGroup>
          </Box>
        </Box>
      }
    >
      <ProxyGroups mode={curMode!} />
    </BasePage>
  );
};

export default ProxyPage;
