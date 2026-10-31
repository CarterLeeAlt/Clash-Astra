import { atom } from "recoil";

// 按订阅标识保存更新中的交互状态。
export const atomLoadingCache = atom<Record<string, boolean>>({
  key: "atomLoadingCache",
  default: {},
});
