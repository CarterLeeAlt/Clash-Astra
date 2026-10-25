import { readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const expected = {
  packageName: "clash-astra",
  displayName: "Clash Astra",
  identifier: "io.github.clash-astra",
  devIdentifier: "io.github.clash-astra.dev",
  mainExe: "Clash Astra.exe",
  watchdogExe: "Clash Astra Watchdog.exe",
  serviceName: "clash-astra-service",
  serviceDisplayName: "Clash Astra Service",
  serviceCrate: "clash-astra-service-src",
  serviceExe: "clash-astra-service.exe",
  serviceInstallExe: "clash-astra-service-install.exe",
  serviceUninstallExe: "clash-astra-service-uninstall.exe",
  firewallHelperExe: "clash-astra-firewall-helper.exe",
  registryKey: "SOFTWARE\\\\clash-astra",
  repository: "https://github.com/CarterLeeAlt/Clash-Astra",
};

const errors = [];
const read = (relativePath) => readFile(path.join(root, relativePath), "utf8");

const expectEqual = (actual, wanted, label) => {
  if (actual !== wanted) {
    errors.push(`${label}: expected "${wanted}", got "${actual}"`);
  }
};

const packageJson = JSON.parse(await read("package.json"));
const tauriConfig = JSON.parse(await read("src-tauri/tauri.conf.json"));
const cargoText = await read("src-tauri/Cargo.toml");

expectEqual(packageJson.name, expected.packageName, "package.json.name");
expectEqual(
  packageJson.description,
  expected.displayName,
  "package.json.description"
);
expectEqual(packageJson.homepage, expected.repository, "package.json.homepage");
expectEqual(
  packageJson.repository,
  `${expected.repository}.git`,
  "package.json.repository"
);
expectEqual(
  packageJson.metadata?.productName,
  expected.displayName,
  "package.json.metadata.productName"
);
expectEqual(
  packageJson.metadata?.identifier,
  expected.identifier,
  "package.json.metadata.identifier"
);
expectEqual(
  tauriConfig.package?.productName,
  expected.displayName,
  "src-tauri/tauri.conf.json package.productName"
);
expectEqual(
  tauriConfig.package?.version,
  packageJson.version,
  "src-tauri/tauri.conf.json package.version"
);
expectEqual(
  tauriConfig.tauri?.bundle?.identifier,
  expected.identifier,
  "src-tauri/tauri.conf.json bundle.identifier"
);
expectEqual(
  tauriConfig.tauri?.bundle?.shortDescription,
  expected.displayName,
  "src-tauri/tauri.conf.json bundle.shortDescription"
);
expectEqual(
  tauriConfig.tauri?.bundle?.longDescription,
  expected.displayName,
  "src-tauri/tauri.conf.json bundle.longDescription"
);

const cargoPackage =
  cargoText.match(/^\[package\]\r?\n([\s\S]*?)\r?\n(?=\[)/m)?.[1] ?? "";
const cargoField = (name) =>
  cargoPackage.match(new RegExp(`^${name}\\s*=\\s*"([^"]*)"`, "m"))?.[1];
expectEqual(
  cargoField("name"),
  expected.packageName,
  "src-tauri/Cargo.toml package.name"
);
expectEqual(
  cargoField("description"),
  expected.displayName,
  "src-tauri/Cargo.toml package.description"
);
expectEqual(
  cargoField("repository"),
  `${expected.repository}.git`,
  "src-tauri/Cargo.toml package.repository"
);
expectEqual(
  cargoField("default-run"),
  expected.packageName,
  "src-tauri/Cargo.toml package.default-run"
);

const watchdogCargoText = await read("src-tauri/watchdog/Cargo.toml");
const watchdogPackage =
  watchdogCargoText.match(/^\[package\]\r?\n([\s\S]*?)\r?\n(?=\[)/m)?.[1] ?? "";
const watchdogField = (name) =>
  watchdogPackage.match(new RegExp(`^${name}\\s*=\\s*"([^"]*)"`, "m"))?.[1];
expectEqual(
  watchdogField("name"),
  "clash-astra-watchdog",
  "src-tauri/watchdog/Cargo.toml package.name"
);

const serviceCargoText = await read("src-tauri/windows-service-src/Cargo.toml");
const serviceCargoPackage =
  serviceCargoText.match(/^\[package\]\r?\n([\s\S]*?)\r?\n(?=\[)/m)?.[1] ?? "";
const serviceCargoField = (name) =>
  serviceCargoPackage.match(new RegExp(`^${name}\\s*=\\s*"([^"]*)"`, "m"))?.[1];
expectEqual(
  serviceCargoField("name"),
  expected.serviceCrate,
  "src-tauri/windows-service-src/Cargo.toml package.name"
);

const requiredSourceFragments = [
  ["src-tauri/src/utils/dirs.rs", `"${expected.identifier}"`],
  ["src-tauri/src/utils/dirs.rs", `"${expected.devIdentifier}"`],
  ["src-tauri/watchdog/src/main.rs", `"${expected.identifier}"`],
  ["src-tauri/watchdog/build.rs", `"${expected.watchdogExe}"`],
  [
    "src-tauri/windows-service-src/src/bin/clash-astra-service.rs",
    `"${expected.identifier}"`,
  ],
  [
    "src-tauri/windows-service-src/src/bin/clash-astra-service.rs",
    `"${expected.devIdentifier}"`,
  ],
  ["src-tauri/windows-service-src/src/lib.rs", `"${expected.serviceName}"`],
  [
    "src-tauri/windows-service-src/src/lib.rs",
    `"${expected.serviceDisplayName}"`,
  ],
  ["src-tauri/windows-service-src/src/lib.rs", `"${expected.serviceExe}"`],
  [
    "src-tauri/windows-service-src/src/lib.rs",
    `"${expected.serviceInstallExe}"`,
  ],
  [
    "src-tauri/windows-service-src/src/lib.rs",
    `"${expected.serviceUninstallExe}"`,
  ],
  [
    "src-tauri/windows-service-src/src/lib.rs",
    `"${expected.firewallHelperExe}"`,
  ],
  ["src-tauri/src/core/win_service.rs", `"${expected.serviceName}"`],
  ["src-tauri/src/core/win_service.rs", `"${expected.serviceInstallExe}"`],
  ["src-tauri/src/core/win_service.rs", `"${expected.serviceUninstallExe}"`],
  ["src-tauri/src/utils/dirs.rs", `"${expected.serviceExe}"`],
  ["src-tauri/src/utils/dirs.rs", `"${expected.firewallHelperExe}"`],
  [
    "src-tauri/vendor-crates/sysproxy-rs/src/windows.rs",
    `"${expected.registryKey}"`,
  ],
  ["src-tauri/src/config/clash.rs", '"clash-astra"'],
  [
    "src/features/settings/components/mods/tun-viewer.tsx",
    'placeholder="clash-astra"',
  ],
  ["src-tauri/src/watchdog_protocol.rs", `"${expected.watchdogExe}"`],
  ["src-tauri/src/watchdog_protocol.rs", '"--clash-astra-watchdog-child"'],
  ["src-tauri/src/utils/server.rs", '"Local\\\\clash-astra-singleton"'],
  ["scripts/build/build-watchdog.mjs", `"${expected.watchdogExe}"`],
  ["scripts/release/portable.mjs", 'const MACHINE_NAME = "clash-astra";'],
  ["scripts/release/portable.mjs", `"${expected.mainExe}"`],
  ["scripts/release/portable.mjs", `"${expected.watchdogExe}"`],
  [".github/workflows/release-windows.yml", `release/${expected.mainExe}`],
  [".github/workflows/release-windows.yml", `release/${expected.watchdogExe}`],
  ["scripts/sidecars/prepare-runtime.mjs", `"${expected.serviceExe}"`],
  ["src/services/ipc/service.ts", `"${expected.serviceName}"`],
  ["src/features/settings/SettingsPage.tsx", expected.repository],
];

for (const [relativePath, fragment] of requiredSourceFragments) {
  const text = await read(relativePath);
  if (!text.includes(fragment)) {
    errors.push(`${relativePath}: missing ${fragment}`);
  }
}

// portable.mjs 与 prepare-runtime.mjs 的清理列表必须保留旧名，故不参与旧名残留检查。
const buildAndCiFiles = [
  "scripts/build/build-watchdog.mjs",
  "scripts/verify/test-watchdog.ps1",
  ".github/workflows/release-windows.yml",
];
const legacyPatterns = [
  ["clash-verge-buty", /clash-verge-buty/],
  ["Clash Verge Buty", /Clash Verge Buty/],
  ["Clash_Verge_Buty", /Clash_Verge_Buty/],
  ["Clash-Verge-Buty", /Clash-Verge-Buty/],
  ["io.github.clash-verge-buty.data", /io\.github\.clash-verge-buty\.data/],
];

for (const relativePath of buildAndCiFiles) {
  const text = await read(relativePath);
  for (const [label, pattern] of legacyPatterns) {
    if (pattern.test(text)) {
      errors.push(`${relativePath}: uses legacy name ${label}`);
    }
  }
}

// 发布链路里不得再出现旧的用户面文件名；Cargo 的产物名（clash-astra-watchdog.exe）
// 只允许作为构建输入出现，不在检查范围内。
const forbiddenFragments = [
  ["scripts/release/portable.mjs", '"clash-astra.exe"'],
  ["scripts/release/portable.mjs", '"clash-astra-watchdog.exe"'],
  [".github/workflows/release-windows.yml", "release/clash-astra.exe"],
];

for (const [relativePath, fragment] of forbiddenFragments) {
  const text = await read(relativePath);
  if (text.includes(fragment)) {
    errors.push(`${relativePath}: uses legacy file name ${fragment}`);
  }
}

const linkFiles = [
  "package.json",
  "src-tauri/Cargo.toml",
  "src/features/settings/SettingsPage.tsx",
];
const legacyRepository = /CarterLeeAlt\/Clash-Verge-Buty/;
for (const relativePath of linkFiles) {
  const text = await read(relativePath);
  if (legacyRepository.test(text)) {
    errors.push(`${relativePath}: uses legacy repository URL`);
  }
}

if (errors.length > 0) {
  console.error("[FAIL] naming check:");
  for (const error of errors) console.error(` - ${error}`);
  process.exit(1);
}

console.log("[OK] naming check passed");
