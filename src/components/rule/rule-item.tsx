import { styled, Box, Typography } from "@mui/material";

const Item = styled(Box)(({ theme }) => ({
  display: "flex",
  alignItems: "center",
  padding: "8px 10px",
  color: theme.palette.text.primary,
}));

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
        sx={{ flex: "none", minWidth: 30, mr: 2.25, textAlign: "left" }}
      >
        {index}
      </Typography>

      <Typography
        component="span"
        variant="subtitle1"
        color="text.primary"
        sx={{
          flex: "1 1 auto",
          minWidth: 0,
          mr: 2,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          userSelect: "text",
        }}
      >
        {value.payload || "-"}
      </Typography>

      <Typography
        component="span"
        variant="body2"
        color="text.secondary"
        sx={{ flex: "none", width: 110, mr: 2, userSelect: "text" }}
      >
        {value.type}
      </Typography>

      <Typography
        component="span"
        variant="body2"
        color={parseColor(value.proxy)}
        sx={{
          flex: "none",
          width: 140,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          userSelect: "text",
        }}
      >
        {value.proxy}
      </Typography>
    </Item>
  );
};

export default RuleItem;
