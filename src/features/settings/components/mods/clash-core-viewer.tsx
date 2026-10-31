import { forwardRef, useImperativeHandle, useState } from "react";
import {
  BaseDialog,
  DialogRef,
  formatNoticeMessage,
  Notice,
} from "@/components/base";
import { useTranslation } from "react-i18next";
import { useLockFn } from "ahooks";
import { RestartAlt } from "@mui/icons-material";
import { Box, Button, Typography } from "@mui/material";
import { restartSidecar } from "@/services/cmds";
import coreLock from "@root/src-tauri/mihomo.lock.json";

export const ClashCoreViewer = forwardRef<DialogRef>((_props, ref) => {
  const { t } = useTranslation();

  const [open, setOpen] = useState(false);

  useImperativeHandle(ref, () => ({
    open: () => setOpen(true),
    close: () => setOpen(false),
  }));

  const onRestart = useLockFn(async () => {
    try {
      await restartSidecar();
      Notice.success(`Mihomo core restarted.`);
    } catch (err: any) {
      Notice.error(formatNoticeMessage(err));
    }
  });

  return (
    <BaseDialog
      open={open}
      title={
        <Box display="flex" justifyContent="space-between">
          {t("Mihomo Core")}
          <Box>
            <Button
              variant="contained"
              size="small"
              onClick={onRestart}
              startIcon={<RestartAlt />}
            >
              {t("Restart")}
            </Button>
          </Box>
        </Box>
      }
      contentSx={{
        pb: 0,
        width: 400,
        overflowY: "auto",
        userSelect: "text",
        marginTop: "-8px",
      }}
      disableOk
      cancelBtn={t("Back")}
      onClose={() => setOpen(false)}
      onCancel={() => setOpen(false)}
    >
      <Typography sx={{ py: 2 }}>Mihomo {coreLock.version}</Typography>
      <Typography variant="body2" color="text.secondary" sx={{ pb: 2 }}>
        {t("Pinned Core Description")}
      </Typography>
    </BaseDialog>
  );
});
