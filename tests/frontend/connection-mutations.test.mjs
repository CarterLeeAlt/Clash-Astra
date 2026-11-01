import assert from "node:assert/strict";
import fs from "node:fs";
import { test } from "node:test";

const source = (file) =>
  fs.readFileSync(new URL(`../../${file}`, import.meta.url), "utf8");

test("manual connection closures use backend readback rather than bare HTTP deletion", () => {
  const endpoints = source("src/infrastructure/mihomo/endpoints.ts");
  assert.match(endpoints, /await closeConnectionsVerified\(id\)/);
  assert.match(endpoints, /await closeConnectionsVerified\(\)/);
  assert.doesNotMatch(endpoints, /instance\.delete/);
  assert.match(
    source("src/infrastructure/ipc/clash.ts"),
    /invoke<void>\("close_connections_verified", \{ id \}\)/
  );
  assert.match(
    source("src-tauri/src/main.rs"),
    /cmds::close_connections_verified/
  );
});

test("connection UI preserves unconfirmed state and reports closure failures", () => {
  const page = source("src/features/connections/ConnectionsPage.tsx");
  assert.doesNotMatch(page, /resetConnections\(/);
  assert.match(
    page,
    /await closeAllConnections\(\);\s+await syncConnections\(\);/
  );
  const detail = source(
    "src/features/connections/components/connection-detail.tsx"
  );
  assert.match(
    detail,
    /await deleteConnection\(data\.id\);\s+onClose\?\.\(\);/
  );
  for (const file of [
    page,
    detail,
    source("src/features/connections/components/connection-item.tsx"),
  ]) {
    assert.match(file, /Notice\.error\(formatNoticeMessage\(error\)\)/);
  }
});
