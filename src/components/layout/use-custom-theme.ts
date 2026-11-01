import { useMemo } from "react";
import { alpha, createTheme, Shadows, Theme } from "@mui/material";
import { defaultTheme } from "@/app/theme";
import { useClashSettings } from "@/hooks/use-clash-settings";
import { textSizeCssVariables, textTheme } from "@/shared/typography";

/**
 * custom theme
 */
export const useCustomTheme = () => {
  const { clashSettings } = useClashSettings();
  const { theme_setting } = clashSettings ?? {};

  const theme = useMemo(() => {
    const setting = theme_setting || {};
    const dt = defaultTheme;

    let theme: Theme;

    try {
      theme = createTheme({
        ...textTheme,
        breakpoints: {
          values: { xs: 0, sm: 650, md: 900, lg: 1200, xl: 1536 },
        },
        palette: {
          mode: "light",
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
          mode: "light",
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
    const backgroundColor = "#f0f0f0";
    const selectColor = "#f5f5f5";
    const scrollColor = "#90939980";
    const dividerColor = "rgba(0, 0, 0, 0.06)";

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
    let style = document.querySelector("style#clash-theme");
    if (!style) {
      style = document.createElement("style");
      style.id = "clash-theme";
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
  }, [theme_setting]);

  return { theme };
};
