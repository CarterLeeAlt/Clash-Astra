import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import AdmZip from "adm-zip";
import { mihomoLock, sha256 } from "../../scripts/lib/mihomo-lock.mjs";
import { runtimeFileSpecs } from "../../scripts/lib/runtime-resources.mjs";

test("portable bundle reads prepared runtime inputs from a clean build layout", (t) => {
  const root = fileURLToPath(new URL("../..", import.meta.url));
  const scratch = path.join(root, ".tmp", "portable-bundle-tests");
  fs.mkdirSync(scratch, { recursive: true });
  const fixture = fs.mkdtempSync(path.join(scratch, "case-"));
  t.diagnostic(`Portable bundle fixture: ${fixture}`);
  const target = mihomoLock.target;
  const release = path.join(fixture, "src-tauri", "target", target, "release");
  fs.mkdirSync(release, { recursive: true });
  fs.writeFileSync(
    path.join(fixture, "package.json"),
    JSON.stringify({ name: "clash-astra", version: "0.0.0" })
  );
  fs.writeFileSync(
    path.join(fixture, "src-tauri", "tauri.conf.json"),
    JSON.stringify({ package: { version: "0.0.0" } })
  );
  const main = Buffer.from("application fixture");
  const watchdog = Buffer.from("watchdog fixture");
  fs.writeFileSync(path.join(release, "Clash Astra.exe"), main);
  fs.writeFileSync(path.join(release, "Clash Astra Watchdog.exe"), watchdog);

  const resources = new Map();
  for (const spec of runtimeFileSpecs(target)) {
    const destination = path.join(fixture, spec.workspacePath);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    if (spec.archivePath.startsWith("sidecar/")) {
      fs.linkSync(path.join(root, spec.workspacePath), destination);
    } else {
      const data = Buffer.from(`resource fixture: ${spec.archivePath}`);
      fs.writeFileSync(destination, data);
      resources.set(spec.archivePath, data);
    }
  }

  const result = spawnSync(
    process.execPath,
    [path.join(root, "scripts/release/portable.mjs"), target],
    {
      cwd: fixture,
      env: {
        ...process.env,
        SKIP_RELEASE_UPLOAD: "1",
        UNSIGNED_BUILD: "1",
        PORTABLE_LABEL: "",
      },
      encoding: "utf8",
      windowsHide: true,
      timeout: 60000,
    }
  );
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const zip = new AdmZip(
    path.join(
      fixture,
      "artifacts/portable/clash-astra_0.0.0_x64_portable_unsigned.zip"
    )
  );
  assert.deepEqual(
    zip
      .getEntries()
      .map((entry) => entry.entryName)
      .sort(),
    [
      "Clash Astra.exe",
      "Clash Astra Watchdog.exe",
      "mihomo.exe",
      ...resources.keys(),
    ].sort()
  );
  assert.deepEqual(zip.readFile("Clash Astra.exe"), main);
  assert.deepEqual(zip.readFile("Clash Astra Watchdog.exe"), watchdog);
  assert.equal(sha256(zip.readFile("mihomo.exe")), mihomoLock.binarySha256);
  for (const [name, data] of resources) {
    assert.deepEqual(zip.readFile(name), data, name);
  }
});
