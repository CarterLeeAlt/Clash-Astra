import fs from "node:fs";
import path from "node:path";
import { mihomoLock, verifyMihomo } from "../lib/mihomo-lock.mjs";
import { repoRoot } from "../lib/workspace-env.mjs";

export function prepareMihomo(target = mihomoLock.target) {
  if (target !== mihomoLock.target) {
    throw new Error(`No verified Mihomo asset for ${target}`);
  }

  const binary = path.join(repoRoot, `src-tauri/sidecar/mihomo-${target}.exe`);
  if (!fs.existsSync(binary)) {
    throw new Error(`Prepared Mihomo binary is missing: ${binary}`);
  }

  verifyMihomo(binary);
  return binary;
}
