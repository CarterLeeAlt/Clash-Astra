import type { PointerEvent } from "react";
import type { ThemeOptions } from "@mui/material/styles";
import { textTheme } from "@/shared/typography";

// 浅色模式的默认主题配置。
export const defaultTheme = {
  primary_color: "#007AFF",
  secondary_color: "#FFCC00",
  primary_text: "#000000",
  secondary_text: "rgba(60, 60, 67, 0.60)",
  info_color: "#007AFF",
  error_color: "#FF3B30",
  warning_color: "#FF9500",
  success_color: "#34C759",
  background_color: "#f5f5f5",
  font_family: `"CharterBT", "Noto Sans SC", "twemoji mozilla"`,
};

export const baseTheme = {
  ...textTheme,
  components: {
    ...textTheme.components,
    MuiButtonBase: {
      defaultProps: {
        onPointerEnter: startPointerTransitions,
        onPointerLeave: startPointerTransitions,
      },
    },
  },
} satisfies ThemeOptions;

function startPointerTransitions(event: PointerEvent<HTMLElement>) {
  const startTime = document.timeline.currentTime;
  if (startTime === null) return;

  // 与指针事件所在帧对齐，避免 WebView 等待合成提交时悬停过渡仍停在起点。
  for (const animation of event.currentTarget.getAnimations()) {
    if (animation instanceof CSSTransition && animation.pending) {
      animation.startTime = startTime;
    }
  }
}
