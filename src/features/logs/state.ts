import { atom } from "recoil";

export const atomLogData = atom<ILogItem[]>({
  key: "atomLogData",
  default: [],
});

export const atomLogError = atom<string | null>({
  key: "atomLogError",
  default: null,
});

export const atomLogConnState = atom<"connected" | "reconnecting" | "paused">({
  key: "atomLogConnState",
  default: "reconnecting",
});

export const atomEnableLog = atom<boolean>({
  key: "atomEnableLog",
  effects: [
    ({ setSelf, onSet }) => {
      const key = "enable-log";

      try {
        setSelf(localStorage.getItem(key) !== "false");
      } catch {}

      onSet((newValue, _, isReset) => {
        try {
          if (isReset) {
            localStorage.removeItem(key);
          } else {
            localStorage.setItem(key, newValue.toString());
          }
        } catch {}
      });
    },
  ],
});
