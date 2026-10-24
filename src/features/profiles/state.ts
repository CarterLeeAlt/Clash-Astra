import { atom } from "recoil";

// save the state of each profile item loading
export const atomLoadingCache = atom<Record<string, boolean>>({
  key: "atomLoadingCache",
  default: {},
});
