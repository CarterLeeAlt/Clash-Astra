import { useCallback, useEffect, useState } from "react";
import { ProxySortType } from "./use-filter-sort";
import { useProfiles } from "@/hooks/use-profiles";

export interface HeadState {
  open?: boolean;
  showType: boolean;
  sortType: ProxySortType;
  filterText: string;
  textState: "url" | "filter" | null;
  testUrl: string;
}

type HeadStateStorage = Record<string, Record<string, HeadState>>;

const HEAD_STATE_KEY = "proxy-head-state";
export const DEFAULT_STATE: HeadState = {
  open: false,
  showType: true,
  sortType: 0,
  filterText: "",
  textState: null,
  testUrl: "",
};

export function useHeadStateNew() {
  const { profiles } = useProfiles();
  const current = profiles?.current || "";

  const [snapshot, setState] = useState<{
    profile: string;
    groups: Record<string, HeadState>;
  }>({ profile: "", groups: {} });

  useEffect(() => {
    if (!current) {
      setState({ profile: "", groups: {} });
      return;
    }

    try {
      const data = JSON.parse(
        localStorage.getItem(HEAD_STATE_KEY) || "{}"
      ) as HeadStateStorage;

      const value = data?.[current] || {};

      if (value && typeof value === "object") {
        setState({ profile: current, groups: value });
      } else {
        setState({ profile: current, groups: {} });
      }
    } catch {
      setState({ profile: current, groups: {} });
    }
  }, [current]);

  // 持久化只在已加载当前订阅后执行；状态更新函数保持纯粹，避免 StrictMode 重放副作用。
  useEffect(() => {
    if (!current || snapshot.profile !== current) return;
    try {
      const saved = JSON.parse(localStorage.getItem(HEAD_STATE_KEY) || "{}");
      const data: HeadStateStorage =
        saved && typeof saved === "object" && !Array.isArray(saved)
          ? saved
          : {};
      data[current] = snapshot.groups;
      localStorage.setItem(HEAD_STATE_KEY, JSON.stringify(data));
    } catch (error) {
      console.warn(
        "[proxy] group display preferences could not be saved",
        error
      );
    }
  }, [current, snapshot]);

  const setHeadState = useCallback(
    (groupName: string, obj: Partial<HeadState>) => {
      setState((old) => {
        const groups = old.profile === current ? old.groups : {};
        const state = groups[groupName] || DEFAULT_STATE;
        return {
          profile: current,
          groups: { ...groups, [groupName]: { ...state, ...obj } },
        };
      });
    },
    [current]
  );

  return [
    snapshot.profile === current ? snapshot.groups : {},
    setHeadState,
  ] as const;
}
