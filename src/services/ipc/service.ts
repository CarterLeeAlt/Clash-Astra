import { invoke } from "@tauri-apps/api/tauri";

/// service mode

export async function checkService() {
  try {
    return await invoke<any>("check_service");
  } catch (err: any) {
    return {
      installed: false,
      running: false,
      api_ready: false,
      core_managed: false,
      core_pid: null,
      service_name: "clash-astra-service",
      message: "Clash Astra Service is not installed.",
      state: "service_not_installed",
    };
  }
}

export async function isElevated() {
  return invoke<boolean>("is_elevated");
}

export async function installService() {
  return invoke<void>("install_service");
}

export async function uninstallService() {
  return invoke<void>("uninstall_service");
}
