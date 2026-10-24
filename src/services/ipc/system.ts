import { invoke } from "@tauri-apps/api/tauri";
import { formatNoticeMessage, Notice } from "@/components/base";

export async function openAppDir() {
  return invoke<void>("open_app_dir").catch((err) =>
    Notice.error(formatNoticeMessage(err))
  );
}

export async function openCoreDir() {
  return invoke<void>("open_core_dir").catch((err) =>
    Notice.error(formatNoticeMessage(err))
  );
}

export async function openLogsDir() {
  return invoke<void>("open_logs_dir").catch((err) =>
    Notice.error(formatNoticeMessage(err))
  );
}

export async function openWebUrl(url: string) {
  return invoke<void>("open_web_url", { url });
}

export async function getAppDir() {
  return invoke<string>("get_app_dir");
}

export async function setWindowSizeLocked(locked: boolean) {
  return invoke<void>("set_window_size_locked", { locked });
}

export async function getWindowStyleConfig() {
  return invoke<IWindowStyleConfig>("get_window_style_config");
}

export async function getSystemProxy() {
  return invoke<{
    enable: boolean;
    server: string;
    bypass: string;
  }>("get_sys_proxy");
}

export async function copyIconFile(
  path: string,
  name: "common.png" | "sysproxy.png" | "tun.png"
) {
  return invoke<void>("copy_icon_file", { path, name });
}

export async function invoke_uwp_tool() {
  return invoke<void>("invoke_uwp_tool").catch((err) =>
    Notice.error(formatNoticeMessage(err))
  );
}

export async function reportFrontendError(message: string, stack?: string) {
  return invoke<void>("report_frontend_error", { message, stack });
}
