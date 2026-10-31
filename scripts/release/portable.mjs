import fs from "fs-extra";
import path from "path";
import AdmZip from "adm-zip";
import { createRequire } from "module";
import { getOctokit, context } from "@actions/github";
import { mihomoLock, verifyMihomo } from "../lib/mihomo-lock.mjs";
import { sourceIdentity } from "../lib/source-identity.mjs";

const target = process.argv.slice(2)[0];
// 同版本的本地修复包用标签区分，不覆盖用户已经下载或正在使用的旧包。
const label = process.env.PORTABLE_LABEL || "";
if (label && !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(label)) {
  throw new Error(
    "PORTABLE_LABEL must contain lowercase letters, digits and single hyphens"
  );
}

const MACHINE_NAME = "clash-astra";
const OUTPUT_DIR = path.join("artifacts", "portable");

const ARCH_MAP = {
  "x86_64-pc-windows-msvc": "x64",
  "aarch64-pc-windows-msvc": "arm64",
};

async function getPackageInfo() {
  const packageJsonPath = path.resolve("./package.json");
  const tauriConfigPath = path.resolve("./src-tauri/tauri.conf.json");
  const packageJson = await fs.readJson(packageJsonPath);
  const tauriConfig = await fs.readJson(tauriConfigPath);

  if (packageJson.name !== MACHINE_NAME) {
    throw new Error(
      `package.name must be ${MACHINE_NAME}, got ${packageJson.name}`
    );
  }

  const version = tauriConfig?.package?.version;
  if (!version) {
    throw new Error("package.version not found in src-tauri/tauri.conf.json");
  }

  return { version };
}

/// 构建 Windows 便携 ZIP，可供本机交付或 CI 上传使用。
async function resolvePortable() {
  if (process.platform !== "win32") return;

  if (!target || !ARCH_MAP[target]) {
    throw new Error(`Unsupported or missing target: ${target}`);
  }

  const { version } = await getPackageInfo();

  const targetReleaseDir = `./src-tauri/target/${target}/release`;
  const releaseDir = (await fs.pathExists(targetReleaseDir))
    ? targetReleaseDir
    : "./src-tauri/target/release";
  if (!(await fs.pathExists(releaseDir))) {
    throw new Error(`could not found the release dir: ${targetReleaseDir}`);
  }
  if (releaseDir !== targetReleaseDir) {
    console.warn(
      `[WARN]: target-specific release dir not found, fallback to ${releaseDir}`
    );
  }

  const releaseExeFiles = (await fs.readdir(releaseDir))
    .filter((name) => name.toLowerCase().endsWith(".exe"))
    .sort();

  console.log("[INFO]: target/release exe files:", releaseExeFiles);

  const portableExeName = "Clash Astra.exe";
  const mainExeSource = releaseExeFiles.find(
    (name) => name.toLowerCase() === portableExeName.toLowerCase()
  );
  if (!mainExeSource) {
    throw new Error(
      `Portable main exe not found under ${releaseDir}; expected ${portableExeName} from the Tauri build`
    );
  }

  const watchdogExeName = "Clash Astra Watchdog.exe";
  const watchdogPath = path.join(releaseDir, watchdogExeName);
  if (
    !(await fs.pathExists(watchdogPath)) ||
    !(await fs.stat(watchdogPath)).isFile()
  ) {
    throw new Error(`File not found: ${watchdogPath}`);
  }

  const deniedKeyword = /(setup|setup_unsigned|installer)/i;
  if (deniedKeyword.test(mainExeSource)) {
    throw new Error(`Portable main exe is installer-like: ${mainExeSource}`);
  }

  const exePath = path.join(releaseDir, mainExeSource);

  const mihomoPath = path.join(releaseDir, "mihomo.exe");
  if (!(await fs.pathExists(mihomoPath))) {
    throw new Error(`File not found: ${mihomoPath}`);
  }

  verifyMihomo(mihomoPath);

  const resourcesPath = path.join(releaseDir, "resources");
  if (!(await fs.pathExists(resourcesPath))) {
    throw new Error(`Folder not found: ${resourcesPath}`);
  }

  const obsoleteResourceNames = [
    "clash-verge-service.exe",
    "install-service.exe",
    "uninstall-service.exe",
    "firewall-helper.exe",
    "clash-verge-buty-service.exe",
    "clash-verge-buty-service-install.exe",
    "clash-verge-buty-service-uninstall.exe",
    "clash-verge-buty-firewall-helper.exe",
  ];
  for (const name of obsoleteResourceNames) {
    if (await fs.pathExists(path.join(resourcesPath, name))) {
      throw new Error(
        `Obsolete resource must be removed before packaging: ${name}`
      );
    }
  }

  const zip = new AdmZip();

  zip.addFile(portableExeName, await fs.readFile(exePath));
  zip.addLocalFile(watchdogPath);
  zip.addLocalFile(mihomoPath);
  zip.addFile(
    "mihomo.lock.json",
    Buffer.from(JSON.stringify(mihomoLock, null, 2) + "\n")
  );
  const buildInfo = {
    contract: mihomoLock.contract,
    mihomo: mihomoLock,
    applicationSource: sourceIdentity(process.cwd()),
    generatorSource: process.env.SUBLINK_SOURCE_DIR
      ? sourceIdentity(path.resolve(process.env.SUBLINK_SOURCE_DIR))
      : null,
  };
  zip.addFile(
    "build-info.json",
    Buffer.from(JSON.stringify(buildInfo, null, 2) + "\n")
  );
  zip.addLocalFolder(resourcesPath, "resources");

  const unsignedSuffix = process.env.UNSIGNED_BUILD === "1" ? "_unsigned" : "";
  const labelSuffix = label ? `_${label}` : "";
  const zipFile = `${MACHINE_NAME}_${version}_${ARCH_MAP[target]}_portable${labelSuffix}${unsignedSuffix}.zip`;
  const zipPath = path.join(OUTPUT_DIR, zipFile);
  await fs.ensureDir(OUTPUT_DIR);
  zip.writeZip(zipPath);
  console.log(`[INFO]: portable zip written to ${zipPath}`);

  const zipCheck = new AdmZip(zipPath)
    .getEntries()
    .map((entry) => entry.entryName);
  console.log("[INFO]: portable zip entries:", zipCheck);

  const zipExeEntries = zipCheck.filter((name) =>
    name.toLowerCase().endsWith(".exe")
  );
  const zipEntriesLower = zipCheck.map((name) => name.toLowerCase());
  if (
    !zipExeEntries.some(
      (name) =>
        path.basename(name).toLowerCase() === portableExeName.toLowerCase()
    )
  ) {
    throw new Error(`portable.zip missing main exe: ${portableExeName}`);
  }
  if (!zipCheck.includes(watchdogExeName)) {
    throw new Error(
      `portable.zip missing watchdog exe at root: ${watchdogExeName}`
    );
  }
  if (
    !zipExeEntries.some(
      (name) => path.basename(name).toLowerCase() === "mihomo.exe"
    )
  ) {
    throw new Error("portable.zip missing mihomo.exe");
  }
  if (zipCheck.some((name) => /alpha/i.test(name)))
    throw new Error("Unverified Alpha core in package");
  if (!zipEntriesLower.some((name) => name.startsWith("resources/"))) {
    throw new Error("portable.zip missing resources/ directory");
  }
  if (zipExeEntries.some((name) => deniedKeyword.test(path.basename(name)))) {
    throw new Error(
      `portable.zip contains installer exe: ${zipExeEntries.join(", ")}`
    );
  }

  console.log("[INFO]: create portable zip successfully");

  // push release assets
  if (process.env.SKIP_RELEASE_UPLOAD === "1") {
    console.log("[INFO]: skip release upload by SKIP_RELEASE_UPLOAD=1");
    return;
  }

  if (process.env.GITHUB_TOKEN === undefined) {
    throw new Error("GITHUB_TOKEN is required");
  }

  const options = { owner: context.repo.owner, repo: context.repo.repo };
  const github = getOctokit(process.env.GITHUB_TOKEN);
  const tag = process.env.TAG_NAME || `v${version}`;
  console.log("[INFO]: upload to ", tag);

  const { data: release } = await github.rest.repos.getReleaseByTag({
    ...options,
    tag,
  });

  const assets = release.assets.filter((x) => x.name === zipFile);
  if (assets.length > 0) {
    const id = assets[0].id;
    await github.rest.repos.deleteReleaseAsset({
      ...options,
      asset_id: id,
    });
  }

  console.log(release.name);

  await github.rest.repos.uploadReleaseAsset({
    ...options,
    release_id: release.id,
    name: zipFile,
    data: zip.toBuffer(),
  });
}

resolvePortable().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
