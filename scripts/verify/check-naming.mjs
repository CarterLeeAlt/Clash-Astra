import { readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const expected = {
  packageName: "clash-verge-buty",
  displayName: "Clash Verge Buty",
  identifier: "io.github.clash-verge-buty",
  devIdentifier: "io.github.clash-verge-buty.dev",
  mainExe: "clash-verge-buty.exe",
  serviceName: "clash-verge-buty-service",
  serviceDisplayName: "Clash Verge Buty Service",
  serviceCrate: "clash-verge-buty-service-src",
  serviceExe: "clash-verge-buty-service.exe",
  serviceInstallExe: "clash-verge-buty-service-install.exe",
  serviceUninstallExe: "clash-verge-buty-service-uninstall.exe",
  firewallHelperExe: "clash-verge-buty-firewall-helper.exe",
  registryKey: "SOFTWARE\\\\clash-verge-buty",
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
  [
    "src-tauri/windows-service-src/src/bin/clash-verge-buty-service.rs",
    `"${expected.identifier}"`,
  ],
  [
    "src-tauri/windows-service-src/src/bin/clash-verge-buty-service.rs",
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
  ["src-tauri/src/config/clash.rs", '"clash-verge-buty"'],
  [
    "src/components/setting/mods/tun-viewer.tsx",
    'placeholder="clash-verge-buty"',
  ],
  ["scripts/build/normalize-main-exe.mjs", `"${expected.mainExe}"`],
  ["scripts/release/portable.mjs", 'const MACHINE_NAME = "clash-verge-buty";'],
  ["scripts/sidecars/prepare-runtime.mjs", `"${expected.serviceExe}"`],
];

for (const [relativePath, fragment] of requiredSourceFragments) {
  const text = await read(relativePath);
  if (!text.includes(fragment)) {
    errors.push(`${relativePath}: missing ${fragment}`);
  }
}

const buildAndCiFiles = [
  "scripts/build/normalize-main-exe.mjs",
  "scripts/release/portable.mjs",
  ".github/workflows/release-windows.yml",
  ".github/actions/alpha-windows/action.yml",
];
const legacyPatterns = [
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

if (errors.length > 0) {
  console.error("[FAIL] naming check:");
  for (const error of errors) console.error(` - ${error}`);
  process.exit(1);
}

console.log("[OK] naming check passed");
