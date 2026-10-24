import { atom } from "recoil";

export const atomThemeMode = atom<"light" | "dark">({
  key: "atomThemeMode",
  default: "light",
});

export * from "@/features/logs/state";
export * from "@/features/connections/state";
export * from "@/features/profiles/state";
