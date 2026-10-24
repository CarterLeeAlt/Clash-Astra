import { TextField, type SxProps, type Theme } from "@mui/material";
import { useTranslation } from "react-i18next";

interface Props {
  value: string;
  onChange: (value: string) => void;
  autoFocus?: boolean;
  fullWidth?: boolean;
  placeholder?: string;
  sx?: SxProps<Theme>;
}

const baseSx = {
  input: { py: 0.65, px: 1.25 },
} as SxProps<Theme>;

/** 日志、连接、规则、代理页共用的过滤输入框。 */
export const FilterTextField = ({
  value,
  onChange,
  autoFocus,
  fullWidth = true,
  placeholder,
  sx,
}: Props) => {
  const { t } = useTranslation();

  return (
    <TextField
      autoFocus={autoFocus}
      hiddenLabel
      fullWidth={fullWidth}
      size="small"
      autoComplete="off"
      spellCheck="false"
      variant="outlined"
      placeholder={placeholder ?? t("Filter conditions")}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      sx={(sx ? [baseSx, sx] : baseSx) as SxProps<Theme>}
    />
  );
};
