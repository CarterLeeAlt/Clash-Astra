import path from "node:path";
import { mihomoLock, verifyMihomo } from "../lib/mihomo-lock.mjs";
import { repoRoot } from "../lib/workspace-env.mjs";
verifyMihomo(
  path.join(repoRoot, `src-tauri/sidecar/mihomo-${mihomoLock.target}.exe`)
);
console.log(
  `[OK] verified Mihomo ${mihomoLock.version}: ${mihomoLock.contract}`
);
