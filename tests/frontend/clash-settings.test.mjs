import assert from "node:assert/strict";
import fs from "node:fs";
import { test } from "node:test";
import { queryKeys } from "../../src/shared/query/keys.ts";

const read = (name) =>
  fs.readFileSync(new URL(`../../${name}`, import.meta.url), "utf8");

test("application settings have their own query and IPC contract", () => {
  assert.equal(queryKeys.clashSettings, "getClashSettings");
  assert.notEqual(queryKeys.clashSettings, queryKeys.clashConfig);
  const ipc = read("src/infrastructure/ipc/clash-settings.ts");
  const commands = read("src-tauri/src/commands/clash_settings.rs");
  const registry = read("src-tauri/src/main.rs");
  for (const command of ["get_clash_settings", "patch_clash_settings"]) {
    assert.ok(ipc.includes(`"${command}"`));
    assert.ok(commands.includes(`fn ${command}(`));
    assert.ok(registry.includes(`cmds::${command}`));
  }
  assert.match(
    read("src-tauri/src/utils/dirs.rs"),
    /CLASH_SETTINGS_FILE[^\n]*"clash-settings\.yaml"/
  );
});

test("native runtime events and frontend listeners share the same names", () => {
  const native = read("src-tauri/src/core/handle.rs");
  const listeners = [
    "src/app/use-app-events.ts",
    "src/features/logs/use-log-stream.ts",
    "src/features/proxies/use-delay-refresh.ts",
  ]
    .map(read)
    .join("\n");
  for (const event of [
    "refresh-clash-config",
    "notice-message",
    "auto-delay-finished",
    "auto-site-test-results",
    "auto-site-test-result",
    "app-log",
  ]) {
    const name = `clash://${event}`;
    assert.ok(native.includes(`"${name}"`), name);
    assert.ok(listeners.includes(`"${name}"`), name);
  }
  for (const file of [
    "src/features/proxies/components/proxy-groups.tsx",
    "src/features/connections/use-connections-stream.ts",
  ]) {
    assert.ok(read(file).includes('"clash://connections-refresh"'));
  }
});
