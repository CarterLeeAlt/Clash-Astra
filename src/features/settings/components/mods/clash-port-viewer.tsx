import { forwardRef, useImperativeHandle, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLockFn } from "ahooks";
import { List, ListItem, ListItemText, TextField } from "@mui/material";
import { useClashInfo } from "@/hooks/use-clash";
import {
  BaseDialog,
  DialogRef,
  formatNoticeMessage,
  Notice,
} from "@/components/base";
import { useVerge } from "@/hooks/use-verge";
import { patchPorts } from "@/services/cmds";

export const ClashPortViewer = forwardRef<DialogRef>((_props, ref) => {
  const { t } = useTranslation();

  const { clashInfo, mutateInfo } = useClashInfo();
  const { verge, mutateVerge } = useVerge();

  const [open, setOpen] = useState(false);
  const [mixedPort, setMixedPort] = useState(
    verge?.verge_mixed_port ?? clashInfo?.mixed_port ?? 7897
  );
  const [socksPort, setSocksPort] = useState(
    verge?.verge_socks_port ?? clashInfo?.socks_port ?? 7898
  );
  const [port, setPort] = useState(
    verge?.verge_port ?? clashInfo?.port ?? 7899
  );

  useImperativeHandle(ref, () => ({
    open: () => {
      if (verge?.verge_mixed_port) setMixedPort(verge?.verge_mixed_port);
      if (verge?.verge_socks_port) setSocksPort(verge?.verge_socks_port);
      if (verge?.verge_port) setPort(verge?.verge_port);
      setOpen(true);
    },
    close: () => setOpen(false),
  }));

  const onSave = useLockFn(async () => {
    if (
      mixedPort === verge?.verge_mixed_port &&
      socksPort === verge?.verge_socks_port &&
      port === verge?.verge_port
    ) {
      setOpen(false);
      return;
    }
    try {
      await patchPorts({ mixed: mixedPort, socks: socksPort, http: port });
      await Promise.all([mutateInfo(), mutateVerge()]);
      setOpen(false);
      Notice.success("Clash ports updated.");
    } catch (err: any) {
      Notice.error(formatNoticeMessage(err));
    }
  });

  return (
    <BaseDialog
      open={open}
      title={t("Clash Port")}
      contentSx={{ width: 300 }}
      okBtn={t("Save")}
      cancelBtn={t("Cancel")}
      onClose={() => setOpen(false)}
      onCancel={() => setOpen(false)}
      onOk={onSave}
    >
      <List>
        <ListItem sx={{ padding: "5px 2px" }}>
          <ListItemText primary="Mixed Port" />
          <TextField
            size="small"
            autoComplete="off"
            sx={{ width: 135 }}
            value={mixedPort}
            onChange={(e) =>
              setMixedPort(+e.target.value?.replace(/\D+/, "").slice(0, 5))
            }
          />
        </ListItem>
        <ListItem sx={{ padding: "5px 2px" }}>
          <ListItemText primary="Socks Port" />
          <TextField
            size="small"
            autoComplete="off"
            sx={{ width: 135 }}
            value={socksPort}
            onChange={(e) =>
              setSocksPort(+e.target.value?.replace(/\D+/, "").slice(0, 5))
            }
          />
        </ListItem>
        <ListItem sx={{ padding: "5px 2px" }}>
          <ListItemText primary="Http Port" />
          <TextField
            size="small"
            autoComplete="off"
            sx={{ width: 135 }}
            value={port}
            onChange={(e) =>
              setPort(+e.target.value?.replace(/\D+/, "").slice(0, 5))
            }
          />
        </ListItem>
      </List>
    </BaseDialog>
  );
});
