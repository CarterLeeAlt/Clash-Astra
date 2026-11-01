import { Box, styled } from "@mui/material";

export const ProfileBox = styled(Box)(
  ({ theme, "aria-selected": selected }) => {
    const { primary, text } = theme.palette;

    const h2color = "#000000";

    // 选中态用 inset 阴影画左侧强调条：不改变盒子尺寸和位置，
    // 卡片外边缘始终落在网格的 10px 基准线上（负 margin 会让卡片左移）。
    const selectionAccent = selected
      ? { boxShadow: `inset 3px 0 0 ${primary.main}` }
      : {};

    return {
      position: "relative",
      display: "block",
      cursor: "pointer",
      textAlign: "left",
      padding: "8px 16px",
      boxSizing: "border-box",
      backgroundColor: "#ffffff",
      ...selectionAccent,
      borderRadius: "8px",
      color: text.secondary,
      "& h2": { color: h2color },
    };
  }
);
