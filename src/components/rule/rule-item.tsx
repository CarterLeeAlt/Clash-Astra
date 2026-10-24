import { styled, Box, Typography } from "@mui/material";

const Item = styled(Box)(({ theme }) => ({
  display: "flex",
  alignItems: "baseline",
  padding: "6px 10px",
  color: theme.palette.text.primary,
}));

/// 四列固定行高，保证序号与正文在视觉上垂直居中
const ROW_LINE_HEIGHT = "22px";

const COLOR = [
  "primary",
  "secondary",
  "info.main",
  "warning.main",
  "success.main",
];

interface Props {
  index: number;
  value: IRuleItem;
}

const parseColor = (text: string) => {
  if (text === "REJECT") return "error.main";
  if (text === "DIRECT") return "text.primary";

  let sum = 0;
  for (let i = 0; i < text.length; i++) {
    sum += text.charCodeAt(i);
  }
  return COLOR[sum % COLOR.length];
};

const RuleItem = (props: Props) => {
  const { index, value } = props;

  return (
    <Item>
      <Typography
        color="text.secondary"
        variant="body2"
        sx={{
          flex: "none",
          width: "32px",
          mr: 1,
          textAlign: "left",
          lineHeight: ROW_LINE_HEIGHT,
        }}
      >
        {index}
      </Typography>

      <Typography
        component="span"
        variant="subtitle1"
        color="text.primary"
        sx={{
          flex: "none",
          width: "260px",
          mr: 2,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          lineHeight: ROW_LINE_HEIGHT,
          userSelect: "text",
        }}
      >
        {value.payload || "-"}
      </Typography>

      <Typography
        component="span"
        variant="body2"
        color="text.secondary"
        sx={{
          flex: "none",
          width: "110px",
          mr: 2,
          lineHeight: ROW_LINE_HEIGHT,
          userSelect: "text",
        }}
      >
        {value.type}
      </Typography>

      <Typography
        component="span"
        variant="body2"
        color={parseColor(value.proxy)}
        sx={{
          flex: "none",
          width: "150px",
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          lineHeight: ROW_LINE_HEIGHT,
          userSelect: "text",
        }}
      >
        {value.proxy}
      </Typography>
    </Item>
  );
};

export default RuleItem;
