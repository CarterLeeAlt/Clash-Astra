import assert from "node:assert/strict";
import { test } from "node:test";
import fs from "node:fs";
import path from "node:path";
import {
  mihomoLock,
  sha256,
  verifyMihomo,
} from "../../scripts/lib/mihomo-lock.mjs";
import { extractPinnedMihomo } from "../../scripts/sidecars/prepare-mihomo.mjs";

test("locked core rejects substituted data and the packaging route has no latest lookup", () => {
  assert.equal(mihomoLock.version, "v1.19.33");
  assert.throws(
    () => extractPinnedMihomo(Buffer.from("not-the-pinned-archive")),
    /SHA-256/
  );
  assert.throws(
    () => verifyMihomo(path.resolve("package.json")),
    /does not match/
  );
  verifyMihomo(
    path.resolve(`src-tauri/sidecar/mihomo-${mihomoLock.target}.exe`)
  );
  const source = fs.readFileSync(
    path.resolve("scripts/sidecars/prepare-runtime.mjs"),
    "utf8"
  );
  assert.doesNotMatch(
    source,
    /getLatest|mihomo-alpha|Prerelease-Alpha|mihomo\/releases\/latest/
  );
  assert.equal(
    sha256(
      fs.readFileSync(
        path.resolve(`src-tauri/sidecar/mihomo-${mihomoLock.target}.exe`)
      )
    ),
    mihomoLock.binarySha256
  );
});
