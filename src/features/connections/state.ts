import { atom } from "recoil";

interface IConnectionSetting {
  layout: "table" | "list";
}

export const atomConnectionSetting = atom<IConnectionSetting>({
  key: "atomConnectionSetting",
  effects: [
    ({ setSelf, onSet }) => {
      const key = "connections-setting";

      try {
        const value = localStorage.getItem(key);
        const data = value == null ? { layout: "table" } : JSON.parse(value);
        setSelf(data);
      } catch {
        setSelf({ layout: "table" });
      }

      onSet((newValue) => {
        try {
          localStorage.setItem(key, JSON.stringify(newValue));
        } catch {}
      });
    },
  ],
});
