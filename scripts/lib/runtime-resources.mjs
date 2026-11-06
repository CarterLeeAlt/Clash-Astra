import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { mihomoLock } from "./mihomo-lock.mjs";
import { repoRoot } from "./workspace-env.mjs";

export const RUNTIME_LOCK_PATH = path.join(
  repoRoot,
  "src-tauri",
  "clash-astra-runtime.lock.json"
);

export const runtimeFileSpecs = (target = mihomoLock.target) => [
  {
    archivePath: `sidecar/mihomo-${target}.exe`,
    workspacePath: path.join("src-tauri", "sidecar", `mihomo-${target}.exe`),
  },
  ...[
    "Country.mmdb",
    "geoip.dat",
    "geosite.dat",
    "enableLoopback.exe",
    "clash-astra-service.exe",
    "clash-astra-service-install.exe",
    "clash-astra-service-uninstall.exe",
    "clash-astra-firewall-helper.exe",
  ].map((file) => ({
    archivePath: `resources/${file}`,
    workspacePath: path.join("src-tauri", "resources", file),
  })),
];

export function readRuntimeLock() {
  return JSON.parse(fs.readFileSync(RUNTIME_LOCK_PATH, "utf8"));
}

export function sha256(data) {
  return createHash("sha256").update(data).digest("hex");
}

export function runtimeReleaseBaseUrl(lock) {
  return `https://github.com/CarterLeeAlt/Clash-Astra/releases/download/${lock.releaseTag}`;
}

export function fileRecordsFromDirectory(root, specs = runtimeFileSpecs()) {
  const files = {};
  for (const spec of specs) {
    const filePath = path.join(root, spec.workspacePath);
    const data = fs.readFileSync(filePath);
    files[spec.archivePath] = {
      sha256: sha256(data),
      size: data.length,
    };
  }
  return files;
}

export function verifyRuntimeFiles(root, expectedFiles) {
  for (const [archivePath, expected] of Object.entries(expectedFiles)) {
    const filePath = path.join(root, archivePath.replaceAll("/", path.sep));
    if (!fs.existsSync(filePath)) {
      throw new Error(`Missing runtime resource: ${archivePath}`);
    }
    const data = fs.readFileSync(filePath);
    const actualSha256 = sha256(data);
    if (actualSha256 !== expected.sha256) {
      throw new Error(
        `Runtime resource SHA-256 mismatch: ${archivePath} expected=${expected.sha256} actual=${actualSha256}`
      );
    }
    if (data.length !== expected.size) {
      throw new Error(
        `Runtime resource size mismatch: ${archivePath} expected=${expected.size} actual=${data.length}`
      );
    }
  }
}

export function verifyRuntimeWorkspace(
  root,
  expectedFiles,
  specs = runtimeFileSpecs()
) {
  for (const spec of specs) {
    const expected = expectedFiles[spec.archivePath];
    if (!expected) {
      throw new Error(`Runtime lock is missing: ${spec.archivePath}`);
    }
    const filePath = path.join(root, spec.workspacePath);
    if (!fs.existsSync(filePath)) {
      throw new Error(`Missing runtime resource: ${spec.archivePath}`);
    }
    const data = fs.readFileSync(filePath);
    const actualSha256 = sha256(data);
    if (actualSha256 !== expected.sha256) {
      throw new Error(
        `Runtime resource SHA-256 mismatch: ${spec.archivePath} expected=${expected.sha256} actual=${actualSha256}`
      );
    }
    if (data.length !== expected.size) {
      throw new Error(
        `Runtime resource size mismatch: ${spec.archivePath} expected=${expected.size} actual=${data.length}`
      );
    }
  }
}

export function assertRuntimeManifest(manifest, lock) {
  if (manifest.schema !== lock.schema) {
    throw new Error(
      `Runtime manifest schema mismatch: expected=${lock.schema} actual=${manifest.schema}`
    );
  }
  if (manifest.runtimeVersion !== lock.runtimeVersion) {
    throw new Error(
      `Runtime version mismatch: expected=${lock.runtimeVersion} actual=${manifest.runtimeVersion}`
    );
  }
  if (manifest.target !== lock.target) {
    throw new Error(
      `Runtime target mismatch: expected=${lock.target} actual=${manifest.target}`
    );
  }
  const manifestPaths = Object.keys(manifest.files).sort();
  const lockPaths = Object.keys(lock.files).sort();
  if (JSON.stringify(manifestPaths) !== JSON.stringify(lockPaths)) {
    throw new Error("Runtime manifest file lock mismatch");
  }
  for (const archivePath of lockPaths) {
    if (
      manifest.files[archivePath].sha256 !== lock.files[archivePath].sha256 ||
      manifest.files[archivePath].size !== lock.files[archivePath].size
    ) {
      throw new Error(`Runtime manifest file lock mismatch: ${archivePath}`);
    }
  }
  if (
    manifest.mihomo.version !== lock.mihomo.version ||
    manifest.mihomo.contract !== lock.mihomo.contract ||
    manifest.mihomo.commit !== lock.mihomo.commit ||
    manifest.mihomo.binarySha256 !== lock.mihomo.binarySha256
  ) {
    throw new Error("Runtime manifest Mihomo lock mismatch");
  }
}
