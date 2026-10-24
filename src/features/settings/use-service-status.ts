import useSWR from "swr";
import { checkService } from "@/services/cmds";

/**
 * 订阅服务模式状态。
 * enabled 为 false 时不发起查询（非 Windows 平台无需读取服务状态）。
 */
export const useServiceStatus = (enabled = true) =>
  useSWR<any>(enabled ? "checkService" : null, checkService, {
    revalidateIfStale: false,
    shouldRetryOnError: false,
    focusThrottleInterval: 5000, // 5s
  });
