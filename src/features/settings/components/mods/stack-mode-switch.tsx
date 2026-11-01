import { useTranslation } from "react-i18next";
import { Button, ButtonGroup, Tooltip } from "@mui/material";
import { useClashSettings } from "@/hooks/use-clash-settings";
import getSystem from "@/utils/get-system";
import { useServiceStatus } from "../../use-service-status";

const isWIN = getSystem() === "windows";

interface Props {
  value?: string;
  onChange?: (value: string) => void;
}

export const StackModeSwitch = (props: Props) => {
  const { value, onChange } = props;
  const { clashSettings } = useClashSettings();
  const { enable_service_mode } = clashSettings ?? {};
  // 切换 TUN 协议栈。
  const { data: serviceStatus } = useServiceStatus(isWIN);

  const { t } = useTranslation();

  return (
    <Tooltip
      title={
        isWIN && (!serviceStatus?.installed || !enable_service_mode)
          ? t("System and Mixed Can Only be Used in Service Mode")
          : ""
      }
    >
      <ButtonGroup size="small" sx={{ my: "4px" }}>
        <Button
          variant={value?.toLowerCase() === "system" ? "contained" : "outlined"}
          onClick={() => onChange?.("system")}
          disabled={
            isWIN && (!serviceStatus?.installed || !enable_service_mode)
          }
          sx={{ textTransform: "capitalize" }}
        >
          System
        </Button>
        <Button
          variant={value?.toLowerCase() === "gvisor" ? "contained" : "outlined"}
          onClick={() => onChange?.("gvisor")}
          sx={{ textTransform: "capitalize" }}
        >
          gVisor
        </Button>

        <Button
          variant={value?.toLowerCase() === "mixed" ? "contained" : "outlined"}
          onClick={() => onChange?.("mixed")}
          disabled={
            isWIN && (!serviceStatus?.installed || !enable_service_mode)
          }
          sx={{ textTransform: "capitalize" }}
        >
          Mixed
        </Button>
      </ButtonGroup>
    </Tooltip>
  );
};
