import fs from "node:fs";
import path from "node:path";
import { repoRoot } from "../lib/workspace-env.mjs";

const failures = [];
const files = (root) =>
  fs.readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const target = path.join(root, entry.name);
    return entry.isDirectory() ? files(target) : [target];
  });

// 这些规则守住业务归属，避免后续修复重新引入共享草稿或前后端双写。
for (const file of files(path.join(repoRoot, "src-tauri/src")).filter((file) =>
  file.endsWith(".rs")
)) {
  const relative = path.relative(repoRoot, file).replaceAll("\\", "/");
  const source = fs.readFileSync(file, "utf8");
  if (/\.draft\(\)|\.discard\(\)/.test(source))
    failures.push(`${relative}: shared draft API is forbidden`);
  if (
    source.includes("Config::publish(") &&
    !relative.endsWith("application/configuration.rs")
  ) {
    failures.push(
      `${relative}: only the configuration coordinator may publish`
    );
  }
  if (
    relative.includes("/domain/") &&
    /(?:crate::(?:application|infrastructure)|use tauri)/.test(source)
  ) {
    failures.push(
      `${relative}: domain logic must not depend on application or platform adapters`
    );
  }
  if (
    relative.includes("/enhance/") &&
    /Config::|crate::core::handle|Handle::emit/.test(source)
  )
    failures.push(`${relative}: compiler requires explicit inputs`);
}

for (const file of files(path.join(repoRoot, "src")).filter((file) =>
  /\.(ts|tsx)$/.test(file)
)) {
  const relative = path.relative(repoRoot, file).replaceAll("\\", "/");
  const source = fs.readFileSync(file, "utf8");
  if (
    /import\s*\{[^}]*\binvoke\b[^}]*\}\s*from\s*["']@tauri-apps\/api\/tauri["']/.test(
      source
    ) &&
    !relative.endsWith("infrastructure/ipc/client.ts")
  ) {
    failures.push(`${relative}: IPC must use the structured client`);
  }
  if (/\b(updateConfigs|updateProxy)\b/.test(source))
    failures.push(
      `${relative}: persistent core mutations belong to backend operations`
    );
}

if (failures.length) {
  failures.forEach((failure) => console.error(failure));
  process.exitCode = 1;
} else {
  console.log("Architecture boundaries passed.");
}
