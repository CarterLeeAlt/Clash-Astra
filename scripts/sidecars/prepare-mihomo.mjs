import fs from "node:fs";
import path from "node:path";
import AdmZip from "adm-zip";
import fetch from "node-fetch";
import proxyAgent from "https-proxy-agent";
import { mihomoLock, sha256, verifyMihomo } from "../lib/mihomo-lock.mjs";
import { repoRoot } from "../lib/workspace-env.mjs";

export function extractPinnedMihomo(data) {
  if (sha256(data) !== mihomoLock.archiveSha256)
    throw new Error("Pinned Mihomo archive SHA-256 mismatch");
  const zip = new AdmZip(data);
  if (!zip.test()) throw new Error("Pinned Mihomo archive is corrupt");
  const entry = zip
    .getEntries()
    .find((item) => item.entryName === "mihomo-windows-amd64-v2.exe");
  if (!entry || sha256(entry.getData()) !== mihomoLock.binarySha256)
    throw new Error("Pinned Mihomo executable SHA-256 mismatch");
  return entry.getData();
}

export async function prepareMihomo(target = mihomoLock.target) {
  if (target !== mihomoLock.target)
    throw new Error(`No verified Mihomo asset for ${target}`);
  const binary = path.join(repoRoot, `src-tauri/sidecar/mihomo-${target}.exe`);
  if (fs.existsSync(binary)) {
    // 存在的文件也必须通过身份校验，不复用来源不明的内核。
    verifyMihomo(binary);
    return;
  }
  const cache = path.join(repoRoot, ".tmp/mihomo-download");
  fs.mkdirSync(cache, { recursive: true });
  const archive = path.join(cache, mihomoLock.asset);
  if (!fs.existsSync(archive)) {
    const proxy = process.env.HTTPS_PROXY || process.env.HTTP_PROXY;
    const response = await fetch(mihomoLock.url, {
      agent: proxy ? proxyAgent(proxy) : undefined,
      signal: AbortSignal.timeout(120000),
    });
    if (!response.ok)
      throw new Error(`Pinned Mihomo download failed: HTTP ${response.status}`);
    const data = Buffer.from(await response.arrayBuffer());
    if (sha256(data) !== mihomoLock.archiveSha256)
      throw new Error("Pinned Mihomo archive SHA-256 mismatch");
    fs.writeFileSync(archive, data);
  }
  const data = fs.readFileSync(archive);
  const executable = extractPinnedMihomo(data);
  fs.mkdirSync(path.dirname(binary), { recursive: true });
  fs.writeFileSync(binary, executable, { flag: "wx" });
  verifyMihomo(binary);
}
