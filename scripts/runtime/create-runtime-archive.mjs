import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import AdmZip from "adm-zip";
import { mihomoLock, verifyMihomo } from "../lib/mihomo-lock.mjs";
import {
  RUNTIME_LOCK_PATH,
  fileRecordsFromDirectory,
  readRuntimeLock,
  runtimeFileSpecs,
  sha256,
} from "../lib/runtime-resources.mjs";
import { repoRoot } from "../lib/workspace-env.mjs";

const existingLock = fs.existsSync(RUNTIME_LOCK_PATH)
  ? readRuntimeLock()
  : null;
const runtimeVersion =
  process.env.CLASH_ASTRA_RUNTIME_VERSION ||
  existingLock?.runtimeVersion ||
  "v1.0.0";
if (!/^v\d+\.\d+\.\d+$/.test(runtimeVersion)) {
  throw new Error(
    "CLASH_ASTRA_RUNTIME_VERSION must use the form vMAJOR.MINOR.PATCH"
  );
}

const target = process.argv[2] || mihomoLock.target;
if (target !== mihomoLock.target) {
  throw new Error(`No locked Runtime resource set for ${target}`);
}

const specs = runtimeFileSpecs(target);
const sourceRoot = repoRoot;
const outputDir = path.join(repoRoot, "artifacts", "runtime");
const archiveName = `clash-astra-runtime-windows-x64-${runtimeVersion}.zip`;
const manifestName = "clash-astra-runtime.json";
const archivePath = path.join(outputDir, archiveName);
const manifestPath = path.join(outputDir, manifestName);

if (process.platform !== "win32") {
  throw new Error("Clash Astra Runtime archives are generated on Windows only");
}

const serviceFiles = [
  "clash-astra-service.exe",
  "clash-astra-service-install.exe",
  "clash-astra-service-uninstall.exe",
  "clash-astra-firewall-helper.exe",
];
const serviceManifestPath = path.join(
  sourceRoot,
  "src-tauri",
  "windows-service-src",
  "Cargo.toml"
);
const serviceOutputDir = path.join(
  sourceRoot,
  "src-tauri",
  "windows-service-src",
  "target",
  target,
  "release"
);
const serviceResourceDir = path.join(sourceRoot, "src-tauri", "resources");

console.log(
  `[INFO] Building Windows service binaries for Runtime ${runtimeVersion}`
);
execFileSync(
  "cargo",
  [
    "build",
    "--manifest-path",
    serviceManifestPath,
    "--release",
    "--target",
    target,
  ],
  { cwd: sourceRoot, stdio: "inherit" }
);
fs.mkdirSync(serviceResourceDir, { recursive: true });
for (const file of serviceFiles) {
  const builtPath = path.join(serviceOutputDir, file);
  if (!fs.existsSync(builtPath)) {
    throw new Error(`Missing built service binary: ${builtPath}`);
  }
  fs.copyFileSync(builtPath, path.join(serviceResourceDir, file));
}

for (const spec of specs) {
  const sourcePath = path.join(sourceRoot, spec.workspacePath);
  if (!fs.existsSync(sourcePath)) {
    throw new Error(`Missing Runtime input: ${sourcePath}`);
  }
}

const mihomoPath = path.join(
  sourceRoot,
  "src-tauri",
  "sidecar",
  `mihomo-${target}.exe`
);
verifyMihomo(mihomoPath);

const files = fileRecordsFromDirectory(sourceRoot, specs);
const manifest = {
  schema: 1,
  runtimeVersion,
  target,
  mihomo: {
    version: mihomoLock.version,
    contract: mihomoLock.contract,
    commit: mihomoLock.commit,
    binarySha256: mihomoLock.binarySha256,
  },
  files,
};

fs.mkdirSync(outputDir, { recursive: true });
const manifestData = Buffer.from(JSON.stringify(manifest, null, 2) + "\n");
fs.writeFileSync(manifestPath, manifestData);

const zip = new AdmZip();
for (const spec of specs) {
  zip.addLocalFile(
    path.join(sourceRoot, spec.workspacePath),
    path.posix.dirname(spec.archivePath),
    path.posix.basename(spec.archivePath)
  );
}
for (const entry of zip.getEntries()) {
  entry.header.time = new Date(1980, 0, 1, 0, 0, 0);
}
zip.writeZip(archivePath);

const archiveData = fs.readFileSync(archivePath);
const lock = {
  schema: 1,
  runtimeVersion,
  releaseTag: `clash-astra-runtime-${runtimeVersion}`,
  target,
  manifest: manifestName,
  manifestSha256: sha256(manifestData),
  archive: archiveName,
  archiveSha256: sha256(archiveData),
  mihomo: {
    version: mihomoLock.version,
    contract: mihomoLock.contract,
    commit: mihomoLock.commit,
    binarySha256: mihomoLock.binarySha256,
  },
  files,
};

if (
  existingLock &&
  existingLock.runtimeVersion === runtimeVersion &&
  (existingLock.manifestSha256 !== lock.manifestSha256 ||
    existingLock.archiveSha256 !== lock.archiveSha256)
) {
  throw new Error(
    `Runtime resources changed under ${runtimeVersion}; set CLASH_ASTRA_RUNTIME_VERSION to a new Runtime version`
  );
}

fs.writeFileSync(RUNTIME_LOCK_PATH, JSON.stringify(lock, null, 2) + "\n");

const sourceFiles = fileRecordsFromDirectory(sourceRoot, specs);
if (JSON.stringify(sourceFiles) !== JSON.stringify(lock.files)) {
  throw new Error("Runtime source files changed while creating the archive");
}
console.log(`[OK] Runtime manifest written: ${manifestPath}`);
console.log(`[OK] Runtime archive written: ${archivePath}`);
console.log(`[OK] Runtime lock written: ${RUNTIME_LOCK_PATH}`);
console.log(`[INFO] runtimeVersion=${runtimeVersion}`);
console.log(`[INFO] archiveSha256=${lock.archiveSha256}`);
