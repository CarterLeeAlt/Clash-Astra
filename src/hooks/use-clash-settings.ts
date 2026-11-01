import useSWR, { mutate } from "swr";
import {
  getClashSettings,
  patchClashSettings as persistClashSettings,
} from "@/services/cmds";
import { queryKeys } from "@/shared/query/keys";

export const useClashSettings = () => {
  const { data: clashSettings, mutate: mutateClashSettings } = useSWR(
    queryKeys.clashSettings,
    getClashSettings
  );

  const patchClashSettings = async (value: Partial<IClashSettings>) => {
    try {
      await persistClashSettings(value);
    } catch (error) {
      await mutateClashSettings().catch(() => undefined);
      throw error;
    }
    await mutateClashSettings();

    const affectsRuntime =
      value.enable_service_mode !== undefined ||
      value.enable_tun_mode !== undefined ||
      value.enable_system_proxy !== undefined;

    if (affectsRuntime) {
      await Promise.all([
        mutate(queryKeys.clashInfo),
        mutate(queryKeys.clashConfig),
        mutate(queryKeys.runtimeConfig),
        mutate(queryKeys.checkService),
      ]);
    }
  };

  return {
    clashSettings,
    mutateClashSettings,
    patchClashSettings,
  };
};
