import {
  existsSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
} from "node:fs";
import path from "node:path";

const args = process.argv.slice(2);
const targetIndex = args.indexOf("--target");
const target = targetIndex === -1 ? undefined : args[targetIndex + 1];
if (targetIndex !== -1 && !target) {
  throw new Error("--target requires a target triple");
}

const tauriConfig = JSON.parse(
  readFileSync(path.join("src-tauri", "tauri.conf.json"), "utf8")
);
const productName = tauriConfig?.package?.productName;
if (!productName) {
  throw new Error("package.productName not found in src-tauri/tauri.conf.json");
}

const releaseDir = path.join(
  "src-tauri",
  "target",
  ...(target ? [target] : []),
  "release"
);
if (!existsSync(releaseDir)) {
  throw new Error(`Release directory not found: ${releaseDir}`);
}

const destinationName = "clash-verge-buty.exe";
const entries = readdirSync(releaseDir);
const findEntry = (name) =>
  entries.find((entry) => entry.toLowerCase() === name.toLowerCase());
const productExeName = `${productName}.exe`;
const productExe = findEntry(productExeName);
const normalizedExe = findEntry(destinationName);

if (!productExe && !normalizedExe) {
  throw new Error(
    `Main executable not found under ${releaseDir}; expected ${destinationName} or ${productExeName}`
  );
}

// Tauri 先按 productName 生成文件；上一次构建可能已经留下规范化后的旧文件。
// 优先使用本次 Tauri 产物，避免把旧的主程序当成新构建。
const sourceName = productExe ?? normalizedExe;
if (sourceName === destinationName) {
  console.log(
    `Main executable already normalized: ${path.join(releaseDir, sourceName)}`
  );
  process.exit(0);
}

const source = path.join(releaseDir, sourceName);
const destination = path.join(releaseDir, destinationName);
const temporary = path.join(
  releaseDir,
  `.clash-verge-buty.${process.pid}.rename`
);

// Windows 文件系统不区分大小写，直接改大小写可能被当成同名文件；
// 先移到临时名，再落到目标名。目标位置可能还放着上一次的规范化产物。
if (sourceName.toLowerCase() !== destinationName) {
  rmSync(destination, { force: true });
}
renameSync(source, temporary);
renameSync(temporary, destination);
console.log(`Renamed ${source} -> ${destination}`);
