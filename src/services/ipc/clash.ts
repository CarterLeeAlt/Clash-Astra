import dayjs from "dayjs";
import { invoke } from "@tauri-apps/api/tauri";

export async function getClashLogs() {
  const regex = /time="(.+?)"\s+level=(.+?)\s+msg="(.+?)"/;
  const newRegex = /(.+?)\s+(.+?)\s+(.+)/;
  const logs = await invoke<string[]>("get_clash_logs");

  return logs
    .map((log) => {
      const result = log.match(regex);
      if (result) {
        const [_, _time, type, payload] = result;
        const time = dayjs(_time).format("MM-DD HH:mm:ss");
        return { time, type, payload };
      }

      const result2 = log.match(newRegex);
      if (result2) {
        const [_, time, type, payload] = result2;
        return { time, type, payload };
      }
      return null;
    })
    .filter(Boolean) as ILogItem[];
}

export async function getClashInfo() {
  return invoke<IClashInfo | null>("get_clash_info");
}

// Get runtime config which controlled by verge
export async function getRuntimeConfig() {
  return invoke<IConfigData | null>("get_runtime_config");
}

export async function getRuntimeYaml() {
  return invoke<string | null>("get_runtime_yaml");
}

export async function getRuntimeExists() {
  return invoke<string[]>("get_runtime_exists");
}

export async function getRuntimeLogs() {
  return invoke<Record<string, [string, string][]>>("get_runtime_logs");
}

export async function patchClashConfig(payload: Partial<IConfigData>) {
  return invoke<void>("patch_clash_config", { payload });
}

export async function changeClashCore(clashCore: string) {
  return invoke<any>("change_clash_core", { clashCore });
}

export async function restartSidecar() {
  return invoke<void>("restart_sidecar");
}

export async function upgradeCore() {
  return invoke<boolean>("upgrade_core");
}

export async function cmdGetProxyDelay(
  name: string,
  timeout: number,
  url?: string
) {
  return invoke<{ delay: number }>("clash_api_get_proxy_delay", {
    name,
    url,
    timeout,
  });
}

/// 最近一轮后端自动延迟测试的低值结果（节点名 -> 延迟毫秒）
export async function getAutoDelayResults() {
  return invoke<Record<string, number>>("get_auto_delay_results");
}

export interface ITestDelayResult {
  delay: number;
  proxy?: string;
}

export async function cmdTestDelay(url: string) {
  return invoke<ITestDelayResult>("test_delay", { url });
}
