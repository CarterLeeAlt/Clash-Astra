import useSWR, { mutate } from "swr";
import { getVergeConfig, patchVergeConfig } from "@/services/cmds";
import { queryKeys } from "@/shared/query/keys";

export const useVerge = () => {
  const { data: verge, mutate: mutateVerge } = useSWR(
    queryKeys.vergeConfig,
    getVergeConfig
  );

  const patchVerge = async (value: Partial<IVergeConfig>) => {
    try {
      await patchVergeConfig(value);
    } catch (error) {
      await mutateVerge().catch(() => undefined);
      throw error;
    }
    await mutateVerge();

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
    verge,
    mutateVerge,
    patchVerge,
  };
};
