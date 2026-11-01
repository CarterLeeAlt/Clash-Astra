import type { ThemeOptions } from "@mui/material/styles";

/** Application text sizes in CSS pixels at the default 16px root size. */
export const textSizePx = {
  tag: 10,
  caption: 12,
  secondary: 14,
  groupTitle: 15,
  body: 16,
  sectionTitle: 18,
  title: 20,
  brand: 22,
} as const;

type TextRole = keyof typeof textSizePx;
const rootFontSize = 16;

/** UI text uses these roles; Monaco requires the numeric pixel values above. */
export const textSizes = Object.fromEntries(
  Object.entries(textSizePx).map(([role, pixels]) => [
    role,
    `${pixels / rootFontSize}rem`,
  ])
) as Record<TextRole, string>;

/** Stylesheets consume the same values without maintaining a second scale. */
export const textSizeCssVariables = Object.fromEntries(
  Object.entries(textSizes).map(([role, size]) => [
    `--astra-font-${role.replace(
      /[A-Z]/g,
      (letter) => `-${letter.toLowerCase()}`
    )}`,
    size,
  ])
);

export const textTheme = {
  typography: {
    h1: { fontSize: textSizes.title },
    h2: { fontSize: textSizes.title },
    h3: { fontSize: textSizes.title },
    h4: { fontSize: textSizes.title },
    h5: { fontSize: textSizes.title },
    h6: { fontSize: textSizes.title },
    subtitle1: { fontSize: textSizes.body },
    subtitle2: { fontSize: textSizes.secondary },
    body1: { fontSize: textSizes.body },
    body2: { fontSize: textSizes.secondary },
    button: { fontSize: textSizes.secondary },
    caption: { fontSize: textSizes.caption },
    overline: { fontSize: textSizes.caption },
  },
  components: {
    MuiButton: {
      styleOverrides: {
        sizeSmall: { fontSize: textSizes.secondary },
        sizeLarge: { fontSize: textSizes.body },
      },
    },
    MuiChip: {
      styleOverrides: { root: { fontSize: textSizes.secondary } },
    },
    MuiTooltip: {
      styleOverrides: {
        tooltip: { fontSize: textSizes.caption },
        touch: { fontSize: textSizes.secondary },
      },
    },
  },
} satisfies ThemeOptions;
