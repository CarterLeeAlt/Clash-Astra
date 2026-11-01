import React, { ReactNode } from "react";
import { Typography } from "@mui/material";
import { textSizes } from "@/shared/typography";
import { BaseErrorBoundary } from "./base-error-boundary";

interface Props {
  title?: React.ReactNode; // 页面标题。
  header?: React.ReactNode; // 标题旁的操作区域。
  contentStyle?: React.CSSProperties;
  children?: ReactNode;
  full?: boolean;
}

export const BasePage: React.FC<Props> = (props) => {
  const { title, header, contentStyle, full, children } = props;
  return (
    <BaseErrorBoundary>
      <div className="base-page">
        <header data-windrag style={{ userSelect: "none" }}>
          <Typography
            sx={{ fontSize: textSizes.title, fontWeight: 600 }}
            data-windrag
          >
            {title}
          </Typography>

          {header}
        </header>

        <div
          className={full ? "base-container no-padding" : "base-container"}
          style={{ backgroundColor: "#ffffff" }}
        >
          <section
            style={{
              backgroundColor: "var(--background-color)",
            }}
          >
            <div className="base-content" style={contentStyle}>
              {children}
            </div>
          </section>
        </div>
      </div>
    </BaseErrorBoundary>
  );
};
