import {
  alpha,
  ListItem,
  ListItemButton,
  ListItemText,
  ListItemIcon,
} from "@mui/material";
import { useMatch, useResolvedPath, useNavigate } from "react-router-dom";
import { textSizes } from "@/shared/typography";

interface Props {
  to: string;
  children: string;
  icon: React.ReactNode;
}
export const LayoutItem = (props: Props) => {
  const { to, children, icon } = props;

  const resolved = useResolvedPath(to);
  const match = useMatch({ path: resolved.pathname, end: true });
  const navigate = useNavigate();

  return (
    <ListItem sx={{ py: 0.5, maxWidth: 250, mx: "auto", padding: "4px 0px" }}>
      <ListItemButton
        selected={!!match}
        sx={[
          {
            borderRadius: 2,
            marginLeft: 1.25,
            paddingLeft: 1.5,
            paddingRight: 1,
            marginRight: 1.25,
            "& .MuiListItemText-primary": {
              color: "text.primary",
              fontSize: textSizes.body,
              fontWeight: 600,
            },
          },
          ({ palette: { primary } }) => {
            const bgcolor = alpha(primary.main, 0.15);
            const color = "#1f1f1f";

            return {
              "&.Mui-selected": { bgcolor },
              "&.Mui-selected:hover": { bgcolor },
              "&.Mui-selected .MuiListItemText-primary": { color },
            };
          },
        ]}
        onClick={() => navigate(to)}
      >
        <ListItemIcon>{icon}</ListItemIcon>
        <ListItemText
          sx={{ textAlign: "center", marginLeft: "-35px" }}
          primary={children}
        />
      </ListItemButton>
    </ListItem>
  );
};
