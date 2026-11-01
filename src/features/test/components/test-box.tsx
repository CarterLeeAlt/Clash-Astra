import { Box, styled } from "@mui/material";

export const TestBox = styled(Box)(({ theme, "aria-selected": selected }) => {
  const { primary, text } = theme.palette;
  const h2color = selected ? primary.main : text.primary;

  return {
    position: "relative",
    width: "100%",
    display: "block",
    cursor: "pointer",
    textAlign: "left",
    borderRadius: 8,
    boxShadow: theme.shadows[2],
    padding: "8px 16px",
    boxSizing: "border-box",
    backgroundColor: "#ffffff",
    color: text.secondary,
    "& h2": { color: h2color },
  };
});
