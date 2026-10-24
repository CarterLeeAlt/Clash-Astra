import { alpha, Box, styled } from "@mui/material";

export const PROFILE_CARD_TITLE_FONT_SIZE = "1.05rem";

export const ProfileBox = styled(Box)(
  ({ theme, "aria-selected": selected }) => {
    const { mode, primary, text } = theme.palette;
    const key = `${mode}-${!!selected}`;

    const backgroundColor = mode === "light" ? "#ffffff" : "#282A36";

    const color = {
      "light-true": text.secondary,
      "light-false": text.secondary,
      "dark-true": alpha(text.secondary, 0.65),
      "dark-false": alpha(text.secondary, 0.65),
    }[key]!;

    const h2color = "#000000";

    // 选中态用 inset 阴影画左侧强调条：不改变盒子尺寸和位置，
    // 卡片外边缘始终落在网格的 10px 基准线上（负 margin 会让卡片左移）。
    const selectionAccent = {
      "light-true": { boxShadow: `inset 3px 0 0 ${primary.main}` },
      "light-false": {},
      "dark-true": { boxShadow: `inset 3px 0 0 ${primary.main}` },
      "dark-false": {},
    }[key];

    return {
      position: "relative",
      display: "block",
      cursor: "pointer",
      textAlign: "left",
      padding: "8px 16px",
      boxSizing: "border-box",
      backgroundColor,
      ...selectionAccent,
      borderRadius: "8px",
      color,
      "& h2": { color: h2color },
    };
  }
);
