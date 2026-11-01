import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const root = fileURLToPath(
  new URL("../../src-tauri/vendor-crates/sysproxy-rs/", import.meta.url)
);

test("system proxy tests contain no host-writing integration entrypoint", () => {
  const walk = (directory) =>
    fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
      const file = path.join(directory, entry.name);
      return entry.isDirectory() ? walk(file) : [file];
    });
  const integration = walk(path.join(root, "tests")).filter((file) =>
    file.endsWith(".rs")
  );
  assert.ok(integration.length > 0);
  for (const file of integration) {
    const source = fs.readFileSync(file, "utf8");
    assert.doesNotMatch(
      source,
      /\.(?:set_system_proxy(?:_registry)?|restore|restore_if_unchanged|prepare_recovery|record_owned_recovery)\s*\(/,
      file
    );
    assert.doesNotMatch(
      source,
      /\b(?:Command|RegKey|WindowsProxySnapshot)\b/,
      file
    );
  }
});
