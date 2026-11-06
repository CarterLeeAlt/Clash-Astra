import assert from "node:assert/strict";
import { test } from "node:test";
import fs from "node:fs";
import path from "node:path";
import {
  mihomoLock,
  sha256,
  verifyMihomo,
} from "../../scripts/lib/mihomo-lock.mjs";
import { prepareMihomo } from "../../scripts/sidecars/prepare-mihomo.mjs";
import {
  readRuntimeLock,
  runtimeFileSpecs,
  verifyRuntimeWorkspace,
} from "../../scripts/lib/runtime-resources.mjs";

test("locked core and Runtime resources reject substituted data", () => {
  assert.equal(mihomoLock.version, "v1.19.33");
  assert.throws(
    () => verifyMihomo(path.resolve("package.json")),
    /does not match/
  );
  const runtimeLock = readRuntimeLock();
  assert.equal(runtimeLock.runtimeVersion, "v1.0.0");
  verifyRuntimeWorkspace(
    path.resolve("."),
    runtimeLock.files,
    runtimeFileSpecs(runtimeLock.target)
  );
  assert.equal(
    prepareMihomo(),
    path.resolve(`src-tauri/sidecar/mihomo-${mihomoLock.target}.exe`)
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
    /MetaCubeX\/mihomo|meta-rules-dat|uwp-tool|releases\/download\/latest/
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
