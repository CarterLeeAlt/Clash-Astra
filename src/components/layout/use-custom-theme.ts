import { useEffect, useMemo } from "react";
import { useRecoilState } from "recoil";
import { alpha, createTheme, Shadows, Theme } from "@mui/material";
import { appWindow } from "@tauri-apps/api/window";
import { atomThemeMode } from "@/services/states";
import { defaultTheme, defaultDarkTheme } from "@/app/theme";
import { useVerge } from "@/hooks/use-verge";
import { textSizeCssVariables, textTheme } from "@/shared/typography";

/**
 * custom theme
 */
export const useCustomTheme = () => {
  const { verge } = useVerge();
  const { theme_mode, theme_setting } = verge ?? {};
  const [mode, setMode] = useRecoilState(atomThemeMode);

  useEffect(() => {
    const themeMode = ["light", "dark", "system"].includes(theme_mode!)
      ? theme_mode!
      : "light";

    if (themeMode !== "system") {
      setMode(themeMode);
      return;
    }

    appWindow.theme().then((m) => m && setMode(m));
    const unlisten = appWindow.onThemeChanged((e) => setMode(e.payload));

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [theme_mode]);

  const theme = useMemo(() => {
    const setting = theme_setting || {};
    const dt = mode === "light" ? defaultTheme : defaultDarkTheme;

    let theme: Theme;

    try {
      theme = createTheme({
        ...textTheme,
        breakpoints: {
          values: { xs: 0, sm: 650, md: 900, lg: 1200, xl: 1536 },
        },
        palette: {
          mode,
          primary: { main: setting.primary_color || dt.primary_color },
          secondary: { main: setting.secondary_color || dt.secondary_color },
          info: { main: setting.info_color || dt.info_color },
          error: { main: setting.error_color || dt.error_color },
          warning: { main: setting.warning_color || dt.warning_color },
          success: { main: setting.success_color || dt.success_color },
          text: {
            primary: setting.primary_text || dt.primary_text,
            secondary: setting.secondary_text || dt.secondary_text,
          },
          background: {
            paper: dt.background_color,
          },
        },
        shadows: Array(25).fill("none") as Shadows,
        typography: {
          ...textTheme.typography,
          fontFamily: dt.font_family,
        },
      });
    } catch {
      // 用户主题字段无效时，保留可用的基础调色板以保证界面能够渲染。
      theme = createTheme({
        ...textTheme,
        breakpoints: {
          values: { xs: 0, sm: 650, md: 900, lg: 1200, xl: 1536 },
        },
        palette: {
          mode,
          primary: { main: dt.primary_color },
          secondary: { main: dt.secondary_color },
          info: { main: dt.info_color },
          error: { main: dt.error_color },
          warning: { main: dt.warning_color },
          success: { main: dt.success_color },
          text: { primary: dt.primary_text, secondary: dt.secondary_text },
        },
        typography: { ...textTheme.typography, fontFamily: dt.font_family },
      });
    }

    // 同步供非 MUI 组件使用的主题 CSS 变量。
    const backgroundColor = mode === "light" ? "#f0f0f0" : "#2e303d";
    const selectColor = mode === "light" ? "#f5f5f5" : "#d5d5d5";
    const scrollColor = mode === "light" ? "#90939980" : "#54545480";
    const dividerColor =
      mode === "light" ? "rgba(0, 0, 0, 0.06)" : "rgba(255, 255, 255, 0.06)";

    const rootEle = document.documentElement;
    for (const [property, size] of Object.entries(textSizeCssVariables)) {
      rootEle.style.setProperty(property, size);
    }
    rootEle.style.setProperty("--divider-color", dividerColor);
    rootEle.style.setProperty("--background-color", backgroundColor);
    rootEle.style.setProperty("--selection-color", selectColor);
    rootEle.style.setProperty("--scroller-color", scrollColor);
    rootEle.style.setProperty("--primary-main", theme.palette.primary.main);
    rootEle.style.setProperty(
      "--background-color-alpha",
      alpha(theme.palette.primary.main, 0.1)
    );

    // 应用用户配置的主题 CSS。
    let style = document.querySelector("style#verge-theme");
    if (!style) {
      style = document.createElement("style");
      style.id = "verge-theme";
      document.head.appendChild(style!);
    }
    if (style) {
      style.innerHTML = setting.css_injection || "";
    }

    // 同步 SVG 图标颜色。
    const { palette } = theme;

    setTimeout(() => {
      const dom = document.querySelector("#Gradient2");
      if (dom) {
        dom.innerHTML = `
        <stop offset="0%" stop-color="${palette.primary.main}" />
        <stop offset="80%" stop-color="${palette.primary.dark}" />
        <stop offset="100%" stop-color="${palette.primary.dark}" />
        `;
      }
    }, 0);

    return theme;
  }, [mode, theme_setting]);

  return { theme };
};
