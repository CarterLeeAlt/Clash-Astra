import assert from "node:assert/strict";
import fs from "node:fs";
import { test } from "node:test";

test("backend entry builds real frontend assets before compiling default Tauri tests", () => {
  const source = fs.readFileSync(
    new URL("../../scripts/verify/test-backend.mjs", import.meta.url),
    "utf8"
  );
  const frontend = source.indexOf('[packageManager, "run", "web:build"]');
  const verified = source.indexOf(
    '"Frontend assets are ready for Tauri test compilation"'
  );
  const backend = source.indexOf('"backend-tests",\n    "cargo"');
  assert.ok(frontend >= 0 && verified > frontend && backend > verified);
  assert.match(source, /if \(\(await wait\(frontend, 300_000\)\) !== 0\)/);
  assert.match(source, /dist\/index\.html/);
  assert.doesNotMatch(source, /--no-default-features/);
});
