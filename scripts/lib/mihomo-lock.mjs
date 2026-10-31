import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { repoRoot } from "./workspace-env.mjs";

export const mihomoLock = JSON.parse(
  fs.readFileSync(path.join(repoRoot, "src-tauri/mihomo.lock.json"), "utf8")
);
export const sha256 = (data) => createHash("sha256").update(data).digest("hex");

export function verifyMihomo(binary, runVersion = true) {
  if (sha256(fs.readFileSync(binary)) !== mihomoLock.binarySha256) {
    throw new Error(
      `Mihomo does not match ${mihomoLock.version} (${mihomoLock.contract}): ${binary}`
    );
  }
  if (runVersion) {
    const version = execFileSync(binary, ["-v"], {
      windowsHide: true,
      encoding: "utf8",
      timeout: 5000,
    });
    if (!version.includes(` ${mihomoLock.version} `))
      throw new Error("Mihomo version output does not match the lock");
  }
}
