import React, { ReactNode } from "react";
import { textSizes } from "@/shared/typography";
import {
  Box,
  List,
  ListItem,
  ListItemText,
  ListSubheader,
} from "@mui/material";

interface ItemProps {
  label: ReactNode;
  extra?: ReactNode;
  children?: ReactNode;
  secondary?: ReactNode;
}

export const SettingItem: React.FC<ItemProps> = (props) => {
  const { label, extra, children, secondary } = props;

  const primary = !extra ? (
    <Box
      sx={{
        display: "flex",
        alignItems: "center",
        fontSize: textSizes.secondary,
      }}
    >
      <span>{label}</span>
    </Box>
  ) : (
    <Box
      sx={{
        display: "flex",
        alignItems: "center",
        fontSize: textSizes.secondary,
      }}
    >
      <span>{label}</span>
      {extra}
    </Box>
  );

  return (
    <ListItem sx={{ pt: "5px", pb: "5px" }}>
      <ListItemText primary={primary} secondary={secondary} />
      {children}
    </ListItem>
  );
};

export const SettingList: React.FC<{
  title: string;
  children: ReactNode;
}> = (props) => (
  <List>
    <ListSubheader
      sx={[
        {
          background: "transparent",
          fontSize: textSizes.body,
          fontWeight: 600,
        },
        ({ palette }) => {
          return {
            color: palette.text.primary,
          };
        },
      ]}
      disableSticky
    >
      {props.title}
    </ListSubheader>

    {props.children}
  </List>
);
