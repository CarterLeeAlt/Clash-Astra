import useSWR from "swr";
import {
  getProfiles,
  patchProfile,
  patchProfilesConfig,
} from "@/services/cmds";
import { queryKeys } from "@/shared/query/keys";

export const useProfiles = () => {
  const { data: profiles, mutate: mutateProfiles } = useSWR(
    queryKeys.profiles,
    getProfiles
  );

  const patchProfiles = async (value: Partial<IProfilesConfig>) => {
    await patchProfilesConfig(value);
    await mutateProfiles();
  };

  const patchCurrent = async (value: Partial<IProfileItem>) => {
    if (profiles?.current) {
      await patchProfile(profiles.current, value);
      await mutateProfiles();
    }
  };

  return {
    profiles,
    current: profiles?.items?.find((p) => p && p.uid === profiles.current),
    patchProfiles,
    patchCurrent,
    mutateProfiles,
  };
};
