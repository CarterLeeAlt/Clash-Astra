import fs from "fs-extra";
import path from "path";
import AdmZip from "adm-zip";
import fetch from "node-fetch";
import proxyAgent from "https-proxy-agent";
import { execSync } from "child_process";
import { prepareMihomo } from "./prepare-mihomo.mjs";
import { sha256 } from "../lib/mihomo-lock.mjs";
import {
  assertRuntimeManifest,
  readRuntimeLock,
  runtimeFileSpecs,
  runtimeReleaseBaseUrl,
  verifyRuntimeFiles,
  verifyRuntimeWorkspace,
} from "../lib/runtime-resources.mjs";
import { repoRoot } from "../lib/workspace-env.mjs";

const FORCE = process.argv.includes("--force");
const arg1 = process.argv.slice(2).find((arg) => arg !== "--force");
const target =
  arg1 ||
  execSync("rustc -vV")
    .toString()
    .match(/(?<=host: ).+(?=\s*)/g)[0];
const lock = readRuntimeLock();

if (target !== lock.target) {
  throw new Error(`No Runtime resource set for ${target}`);
}

const specs = runtimeFileSpecs(target);
const requiredServiceResources = [
  "clash-astra-service.exe",
  "clash-astra-service-install.exe",
  "clash-astra-service-uninstall.exe",
  "clash-astra-firewall-helper.exe",
];
const specPaths = new Set(specs.map((spec) => spec.archivePath));
for (const file of requiredServiceResources) {
  if (!specPaths.has(`resources/${file}`)) {
    throw new Error(`Runtime lock does not define service resource: ${file}`);
  }
}
const expectedArchivePaths = new Set(Object.keys(lock.files));
const cacheDir = path.join(
  repoRoot,
  ".tmp",
  "clash-astra-runtime",
  lock.runtimeVersion
);

function getHttpProxyAgent() {
  const httpProxy =
    process.env.HTTP_PROXY ||
    process.env.http_proxy ||
    process.env.HTTPS_PROXY ||
    process.env.https_proxy;

  return httpProxy ? proxyAgent(httpProxy) : undefined;
}

function readPositiveNumber(value, fallback) {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

async function fetchWithRetry(url, options = {}, context = {}) {
  const retries = Math.max(
    5,
    Math.floor(
      readPositiveNumber(context.retries ?? process.env.CLASH_DOWNLOAD_RETRY, 5)
    )
  );
  const retryDelayMs = readPositiveNumber(
    context.retryDelayMs ?? process.env.CLASH_DOWNLOAD_RETRY_DELAY_MS,
    3000
  );
  const timeoutMs = readPositiveNumber(
    context.timeoutMs ?? process.env.CLASH_DOWNLOAD_TIMEOUT_MS,
    60000
  );
  let lastError;

  for (let attempt = 1; attempt <= retries; attempt++) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);

    try {
      console.log(
        `[INFO]: fetching attempt ${attempt}/${retries} url="${url}" targetPath="${context.targetPath}"`
      );
      const response = await fetch(url, {
        ...options,
        signal: controller.signal,
      });
      if (!response.ok) {
        throw new Error(
          `request failed status=${response.status} statusText="${response.statusText}"`
        );
      }
      return response;
    } catch (error) {
      lastError = error;
      console.warn(
        `[WARN]: fetch failed attempt ${attempt}/${retries} url="${url}" error="${
          error?.message || error
        }"`
      );
      if (attempt < retries) {
        await new Promise((resolve) =>
          setTimeout(resolve, retryDelayMs * attempt)
        );
      }
    } finally {
      clearTimeout(timer);
    }
  }

  throw lastError;
}

async function downloadPinnedAsset(name, expectedSha256) {
  const targetPath = path.join(cacheDir, name);
  if (await fs.pathExists(targetPath)) {
    const cached = await fs.readFile(targetPath);
    if (sha256(cached) === expectedSha256) return cached;
    await fs.remove(targetPath);
  }

  const url = `${runtimeReleaseBaseUrl(lock)}/${name}`;
  const agent = getHttpProxyAgent();
  const response = await fetchWithRetry(url, agent ? { agent } : {}, {
    targetPath,
  });
  const data = Buffer.from(await response.arrayBuffer());
  if (sha256(data) !== expectedSha256) {
    throw new Error(`Runtime asset SHA-256 mismatch: ${name}`);
  }
  await fs.ensureDir(cacheDir);
  await fs.writeFile(targetPath, data);
  return data;
}

function verifyLocalRuntime() {
  verifyRuntimeWorkspace(repoRoot, lock.files, specs);
  prepareMihomo(target);
}

function extractRuntimeArchive(data, stageRoot) {
  const zip = new AdmZip(data);
  const entries = zip
    .getEntries()
    .filter((entry) => !entry.isDirectory)
    .map((entry) => entry.entryName.replaceAll("\\", "/"));
  const unexpected = entries.filter(
    (entry) => !expectedArchivePaths.has(entry)
  );
  if (unexpected.length) {
    throw new Error(
      `Unexpected Runtime archive files: ${unexpected.join(", ")}`
    );
  }

  for (const archivePath of expectedArchivePaths) {
    const entry = zip.getEntry(archivePath);
    if (!entry) throw new Error(`Missing Runtime archive file: ${archivePath}`);

    const outputPath = path.resolve(
      stageRoot,
      archivePath.replaceAll("/", path.sep)
    );
    const stagePath = path.resolve(stageRoot) + path.sep;
    if (!outputPath.startsWith(stagePath)) {
      throw new Error(`Unsafe Runtime archive path: ${archivePath}`);
    }
    fs.ensureDirSync(path.dirname(outputPath));
    fs.writeFileSync(outputPath, entry.getData());
  }
}

async function installRuntime(stageRoot) {
  for (const spec of specs) {
    const sourcePath = path.join(
      stageRoot,
      spec.archivePath.replaceAll("/", path.sep)
    );
    const targetPath = path.join(repoRoot, spec.workspacePath);
    await fs.ensureDir(path.dirname(targetPath));
    await fs.copyFile(sourcePath, targetPath);
  }
}

async function prepareRuntime() {
  if (!FORCE) {
    try {
      verifyLocalRuntime();
      console.log(
        `[OK] Reused verified Clash Astra Runtime ${lock.runtimeVersion}`
      );
      return;
    } catch (error) {
      console.log(`[INFO] Local Runtime is not ready: ${error.message}`);
    }
  }

  if (process.env.CLASH_ASTRA_RUNTIME_OFFLINE === "1") {
    throw new Error(
      `Clash Astra Runtime ${lock.runtimeVersion} is missing or invalid in offline mode`
    );
  }

  await fs.ensureDir(cacheDir);
  const manifestData = await downloadPinnedAsset(
    lock.manifest,
    lock.manifestSha256
  );
  const manifest = JSON.parse(manifestData.toString("utf8"));
  assertRuntimeManifest(manifest, lock);

  const archiveData = await downloadPinnedAsset(
    lock.archive,
    lock.archiveSha256
  );
  const stageRoot = path.join(cacheDir, "stage");
  await fs.remove(stageRoot);
  await fs.ensureDir(stageRoot);
  extractRuntimeArchive(archiveData, stageRoot);
  verifyRuntimeFiles(stageRoot, lock.files);

  await installRuntime(stageRoot);
  verifyLocalRuntime();
  console.log(
    `[OK] Prepared Clash Astra Runtime ${lock.runtimeVersion} from ${lock.releaseTag}`
  );
}

await prepareRuntime();
