import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const root = fileURLToPath(new URL("../..", import.meta.url));
const read = (name) => fs.readFileSync(path.join(root, name), "utf8");
const files = (directory) =>
  fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const name = path.join(directory, entry.name);
    return entry.isDirectory() ? files(name) : [name];
  });

test("application source has no theme selector, mode state or system-theme listener", () => {
  const obsolete =
    /atomThemeMode|theme_mode|defaultDarkTheme|ThemeModeSwitch|theme\.(?:light|dark|system)|Theme Mode|onThemeChanged|prefers-color-scheme|vs-dark|dark-(?:true|false)/;
  for (const name of files(path.join(root, "src")).filter((name) =>
    /\.(ts|tsx|css|scss|json)$/.test(name)
  )) {
    assert.doesNotMatch(
      fs.readFileSync(name, "utf8"),
      obsolete,
      path.relative(root, name)
    );
  }
  assert.equal(
    fs.existsSync(
      path.join(
        root,
        "src/features/settings/components/mods/theme-mode-switch.tsx"
      )
    ),
    false
  );
  assert.match(
    read("src/components/layout/use-custom-theme.ts"),
    /mode:\s*"light"/
  );
  assert.match(
    read("src/features/settings/components/setting-verge.tsx"),
    /Theme Setting/
  );
});

test("native windows, browser controls and both editors explicitly use the light theme", () => {
  assert.match(
    read("src-tauri/src/window/create.rs"),
    /\.theme\(Some\(tauri::Theme::Light\)\)/
  );
  assert.match(
    read("src/assets/styles/index.scss"),
    /color-scheme:\s*light\s*;/
  );
  for (const name of [
    "src/features/profiles/components/editor-viewer.tsx",
    "src/features/settings/components/mods/config-viewer.tsx",
  ]) {
    assert.match(read(name), /theme:\s*"vs"/);
  }
  assert.doesNotMatch(
    read("src-tauri/src/config/verge.rs"),
    /pub\s+theme_mode\s*:/
  );
});
