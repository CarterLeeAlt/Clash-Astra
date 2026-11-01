import { forwardRef, useEffect, useImperativeHandle, useState } from "react";
import { useTranslation } from "react-i18next";
import { List, Button } from "@mui/material";
import { useClashSettings } from "@/hooks/use-clash-settings";
import {
  BaseDialog,
  DialogRef,
  formatNoticeMessage,
  Notice,
  Switch,
} from "@/components/base";
import { SettingItem } from "./setting-comp";
import { GuardState } from "./guard-state";
import { open as openDialog } from "@tauri-apps/api/dialog";
import { convertFileSrc } from "@tauri-apps/api/tauri";
import { copyIconFile, getAppDir } from "@/services/cmds";
import { join } from "@tauri-apps/api/path";

export const LayoutViewer = forwardRef<DialogRef>((_props, ref) => {
  const { t } = useTranslation();
  const { clashSettings, patchClashSettings, mutateClashSettings } =
    useClashSettings();

  const [open, setOpen] = useState(false);
  const [commonIcon, setCommonIcon] = useState("");
  const [sysproxyIcon, setSysproxyIcon] = useState("");
  const [tunIcon, setTunIcon] = useState("");

  useEffect(() => {
    initIconPath();
  }, []);

  async function initIconPath() {
    const appDir = await getAppDir();
    const icon_dir = await join(appDir, "icons");
    const common_icon = await join(icon_dir, "common.png");
    const sysproxy_icon = await join(icon_dir, "sysproxy.png");
    const tun_icon = await join(icon_dir, "tun.png");
    setCommonIcon(common_icon);
    setSysproxyIcon(sysproxy_icon);
    setTunIcon(tun_icon);
  }

  useImperativeHandle(ref, () => ({
    open: () => setOpen(true),
    close: () => setOpen(false),
  }));

  const onSwitchFormat = (_e: any, value: boolean) => value;
  const onError = (err: any) => {
    Notice.error(formatNoticeMessage(err));
  };
  const onChangeData = (patch: Partial<IClashSettings>) => {
    mutateClashSettings((current) => ({ ...current, ...patch }), false);
  };

  return (
    <BaseDialog
      open={open}
      title={t("Layout Setting")}
      contentSx={{ width: 450 }}
      disableOk
      cancelBtn={t("Cancel")}
      onClose={() => setOpen(false)}
      onCancel={() => setOpen(false)}
    >
      <List>
        <SettingItem label={t("Traffic Graph")}>
          <GuardState
            value={clashSettings?.traffic_graph ?? true}
            valueProps="checked"
            onCatch={onError}
            onFormat={onSwitchFormat}
            onChange={(e) => onChangeData({ traffic_graph: e })}
            onGuard={(e) => patchClashSettings({ traffic_graph: e })}
          >
            <Switch edge="end" />
          </GuardState>
        </SettingItem>

        <SettingItem label={t("Memory Usage")}>
          <GuardState
            value={clashSettings?.enable_memory_usage ?? true}
            valueProps="checked"
            onCatch={onError}
            onFormat={onSwitchFormat}
            onChange={(e) => onChangeData({ enable_memory_usage: e })}
            onGuard={(e) => patchClashSettings({ enable_memory_usage: e })}
          >
            <Switch edge="end" />
          </GuardState>
        </SettingItem>

        <SettingItem label={t("Proxy Group Icon")}>
          <GuardState
            value={clashSettings?.enable_group_icon ?? true}
            valueProps="checked"
            onCatch={onError}
            onFormat={onSwitchFormat}
            onChange={(e) => onChangeData({ enable_group_icon: e })}
            onGuard={(e) => patchClashSettings({ enable_group_icon: e })}
          >
            <Switch edge="end" />
          </GuardState>
        </SettingItem>

        <SettingItem label={t("Common Tray Icon")}>
          <GuardState
            value={clashSettings?.common_tray_icon}
            onCatch={onError}
            onChange={(e) => onChangeData({ common_tray_icon: e })}
            onGuard={(e) => patchClashSettings({ common_tray_icon: e })}
          >
            <Button
              variant="outlined"
              size="small"
              startIcon={
                clashSettings?.common_tray_icon &&
                commonIcon && (
                  <img height="20px" src={convertFileSrc(commonIcon)} />
                )
              }
              onClick={async () => {
                if (clashSettings?.common_tray_icon) {
                  onChangeData({ common_tray_icon: false });
                  patchClashSettings({ common_tray_icon: false });
                } else {
                  const path = await openDialog({
                    directory: false,
                    multiple: false,
                    filters: [
                      {
                        name: "Tray Icon Image",
                        extensions: ["png"],
                      },
                    ],
                  });
                  if (path?.length) {
                    await copyIconFile(`${path}`, "common.png");
                    onChangeData({ common_tray_icon: true });
                    patchClashSettings({ common_tray_icon: true });
                  }
                }
              }}
            >
              {clashSettings?.common_tray_icon ? t("Clear") : t("Browse")}
            </Button>
          </GuardState>
        </SettingItem>

        <SettingItem label={t("System Proxy Tray Icon")}>
          <GuardState
            value={clashSettings?.sysproxy_tray_icon}
            onCatch={onError}
            onChange={(e) => onChangeData({ sysproxy_tray_icon: e })}
            onGuard={(e) => patchClashSettings({ sysproxy_tray_icon: e })}
          >
            <Button
              variant="outlined"
              size="small"
              startIcon={
                clashSettings?.sysproxy_tray_icon &&
                sysproxyIcon && (
                  <img height="20px" src={convertFileSrc(sysproxyIcon)} />
                )
              }
              onClick={async () => {
                if (clashSettings?.sysproxy_tray_icon) {
                  onChangeData({ sysproxy_tray_icon: false });
                  patchClashSettings({ sysproxy_tray_icon: false });
                } else {
                  const path = await openDialog({
                    directory: false,
                    multiple: false,
                    filters: [
                      {
                        name: "Tray Icon Image",
                        extensions: ["png"],
                      },
                    ],
                  });
                  if (path?.length) {
                    await copyIconFile(`${path}`, "sysproxy.png");
                    onChangeData({ sysproxy_tray_icon: true });
                    patchClashSettings({ sysproxy_tray_icon: true });
                  }
                }
              }}
            >
              {clashSettings?.sysproxy_tray_icon ? t("Clear") : t("Browse")}
            </Button>
          </GuardState>
        </SettingItem>

        <SettingItem label={t("Tun Tray Icon")}>
          <GuardState
            value={clashSettings?.tun_tray_icon}
            onCatch={onError}
            onChange={(e) => onChangeData({ tun_tray_icon: e })}
            onGuard={(e) => patchClashSettings({ tun_tray_icon: e })}
          >
            <Button
              variant="outlined"
              size="small"
              startIcon={
                clashSettings?.tun_tray_icon &&
                tunIcon && <img height="20px" src={convertFileSrc(tunIcon)} />
              }
              onClick={async () => {
                if (clashSettings?.tun_tray_icon) {
                  onChangeData({ tun_tray_icon: false });
                  patchClashSettings({ tun_tray_icon: false });
                } else {
                  const path = await openDialog({
                    directory: false,
                    multiple: false,
                    filters: [
                      {
                        name: "Tray Icon Image",
                        extensions: ["png"],
                      },
                    ],
                  });
                  if (path?.length) {
                    await copyIconFile(`${path}`, "tun.png");
                    onChangeData({ tun_tray_icon: true });
                    patchClashSettings({ tun_tray_icon: true });
                  }
                }
              }}
            >
              {clashSettings?.tun_tray_icon ? t("Clear") : t("Browse")}
            </Button>
          </GuardState>
        </SettingItem>
      </List>
    </BaseDialog>
  );
});
