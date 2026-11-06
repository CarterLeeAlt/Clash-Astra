import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import {
  textSizePx,
  textSizes,
  textSizeCssVariables,
  textTheme,
} from "../../src/shared/typography.ts";

const require = createRequire(import.meta.url);
const ts = require("typescript");
const { createTheme } = require("@mui/material/styles");
const sourceRoot = fileURLToPath(new URL("../../src", import.meta.url));

test("the shared text roles are the only source of pixel, rem and CSS variable sizes", () => {
  assert.deepEqual(
    Object.values(textSizePx),
    [10, 12, 13, 14, 14, 15, 16, 18, 20, 22]
  );
  assert.deepEqual(Object.keys(textSizes), Object.keys(textSizePx));
  assert.deepEqual(
    Object.values(textSizeCssVariables),
    Object.values(textSizes)
  );
  for (const [role, pixels] of Object.entries(textSizePx)) {
    assert.equal(textSizes[role], `${pixels / 16}rem`);
  }
  assert.equal(textSizeCssVariables["--astra-font-brand"], textSizes.brand);
  assert.equal(
    textSizeCssVariables["--astra-font-section-title"],
    textSizes.sectionTitle
  );
});

test("MUI text variants and component-specific defaults use the shared scale", () => {
  const theme = createTheme({ ...textTheme, palette: { mode: "light" } });
  for (const [variant, role] of Object.entries({
    h1: "title",
    h2: "title",
    h3: "title",
    h4: "title",
    h5: "title",
    h6: "title",
    subtitle1: "body",
    subtitle2: "secondary",
    body1: "body",
    body2: "secondary",
    button: "secondary",
    caption: "caption",
    overline: "caption",
  }))
    assert.equal(theme.typography[variant].fontSize, textSizes[role]);
  assert.equal(
    theme.components.MuiButton.styleOverrides.sizeSmall.fontSize,
    textSizes.secondary
  );
  assert.equal(
    theme.components.MuiButton.styleOverrides.sizeLarge.fontSize,
    textSizes.body
  );
  assert.equal(
    theme.components.MuiChip.styleOverrides.root.fontSize,
    textSizes.secondary
  );
  assert.equal(
    theme.components.MuiTooltip.styleOverrides.tooltip.fontSize,
    textSizes.caption
  );
  assert.equal(
    theme.components.MuiTooltip.styleOverrides.touch.fontSize,
    textSizes.secondary
  );
});

function inspectTextSizes(name, code) {
  const source = ts.createSourceFile(
    name,
    code,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TSX
  );
  const icons = new Set();
  const iconPropTypes = new Set();
  const sizeObjects = new Set();
  const failures = [];
  for (const statement of source.statements) {
    if (!ts.isImportDeclaration(statement) || !statement.importClause) continue;
    const module = statement.moduleSpecifier.text;
    const imports = statement.importClause.namedBindings;
    if (module.startsWith("@mui/icons-material") && statement.importClause.name)
      icons.add(statement.importClause.name.text);
    if (!imports || !ts.isNamedImports(imports)) continue;
    for (const item of imports.elements) {
      const original = (item.propertyName ?? item.name).text;
      if (
        module.startsWith("@mui/icons-material") ||
        (module === "@mui/material" && original === "SvgIcon")
      )
        icons.add(item.name.text);
      if (module === "@mui/material" && original === "SvgIconProps")
        iconPropTypes.add(item.name.text);
      if (
        module === "@/shared/typography" &&
        ["textSizes", "textSizePx"].includes(original)
      )
        sizeObjects.add(item.name.text);
    }
  }
  // Icon sizing is not text typography; typed shared icon props make that boundary explicit.
  const isIcon = (node) => {
    for (let parent = node.parent; parent; parent = parent.parent) {
      if (ts.isJsxOpeningElement(parent) || ts.isJsxSelfClosingElement(parent))
        return icons.has(parent.tagName.getText(source));
      if (ts.isVariableDeclaration(parent)) {
        const annotation =
          parent.type ??
          (parent.initializer && ts.isSatisfiesExpression(parent.initializer)
            ? parent.initializer.type
            : null);
        if (annotation && iconPropTypes.has(annotation.getText(source)))
          return true;
      }
    }
    return false;
  };
  const visit = (node) => {
    if (
      (ts.isPropertyAssignment(node) || ts.isJsxAttribute(node)) &&
      node.name.getText(source).replaceAll('"', "").replaceAll("'", "") ===
        "fontSize" &&
      !isIcon(node)
    ) {
      let value = node.initializer;
      if (value && ts.isJsxExpression(value)) value = value.expression;
      const inherited =
        value && ts.isStringLiteral(value) && value.text === "inherit";
      const shared =
        value &&
        ts.isPropertyAccessExpression(value) &&
        ts.isIdentifier(value.expression) &&
        sizeObjects.has(value.expression.text) &&
        Object.hasOwn(textSizePx, value.name.text);
      if (!inherited && !shared) {
        const line =
          source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1;
        failures.push(
          `${name}:${line}: text fontSize must reference a shared role`
        );
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  for (const match of code.matchAll(
    /font-size\s*:\s*["']?\s*[+-]?(?:\d|\.\d)/g
  )) {
    const line = code.slice(0, match.index).split("\n").length;
    failures.push(
      `${name}:${line}: stylesheet text size must reference the shared scale`
    );
  }
  return failures;
}

test("the typography audit permits icon sizes and rejects scattered text sizes", () => {
  const imports =
    'import { Typography, type SvgIconProps } from "@mui/material"; import { RefreshRounded } from "@mui/icons-material"; import { textSizes } from "@/shared/typography";';
  assert.deepEqual(
    inspectTextSizes(
      "valid.tsx",
      `${imports} const icon = { sx: { fontSize: 16 } } satisfies SvgIconProps; const view = <><RefreshRounded sx={{ fontSize: 24 }} /><Typography fontSize={textSizes.body} /></>;`
    ),
    []
  );
  assert.equal(
    inspectTextSizes(
      "invalid.tsx",
      `${imports} const view = <Typography sx={{ fontSize: 13 }} />;`
    ).length,
    1
  );
  assert.equal(
    inspectTextSizes("invalid.scss", ".label { font-size: 13px; }").length,
    1
  );
});

test("application text does not declare independent font sizes", () => {
  const files = (directory) =>
    fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
      const file = path.join(directory, entry.name);
      return entry.isDirectory() ? files(file) : [file];
    });
  const failures = files(sourceRoot)
    .filter((file) => /\.(ts|tsx|css|scss)$/.test(file))
    .flatMap((file) => {
      const relative = path
        .relative(sourceRoot, file)
        .split(path.sep)
        .join("/");
      return relative === "shared/typography.ts"
        ? []
        : inspectTextSizes(relative, fs.readFileSync(file, "utf8"));
    });
  assert.deepEqual(failures, []);
});
