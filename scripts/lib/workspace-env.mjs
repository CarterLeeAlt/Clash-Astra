import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const repoRoot = path.resolve(
  fileURLToPath(new URL("../..", import.meta.url))
);

export const workspacePaths = {
  toolchain: path.join(repoRoot, ".toolchain"),
  pnpmStore: path.join(repoRoot, ".toolchain", "pnpm-store"),
  pnpmCache: path.join(repoRoot, ".toolchain", "npm-cache"),
  pnpmHome: path.join(repoRoot, ".toolchain", "pnpm-home"),
  pnpmState: path.join(repoRoot, ".toolchain", "pnpm-state"),
  xdgCache: path.join(repoRoot, ".toolchain", "xdg-cache"),
  xdgState: path.join(repoRoot, ".toolchain", "xdg-state"),
  xdgData: path.join(repoRoot, ".toolchain", "xdg-data"),
  xdgConfig: path.join(repoRoot, ".toolchain", "xdg-config"),
  cargoHome: path.join(repoRoot, ".cargo"),
  rustupHome: path.join(repoRoot, ".rustup"),
  hostTemp: path.join(repoRoot, ".tmp", "host-temp"),
  nodeModules: path.join(repoRoot, "node_modules"),
};

const PNPM_CJS = path.join(
  workspacePaths.toolchain,
  "pnpm",
  "node_modules",
  "pnpm",
  "bin",
  "pnpm.cjs"
);

export function findLocalNode() {
  if (!fs.existsSync(workspacePaths.toolchain)) return null;

  const candidates = [];
  for (const entry of fs.readdirSync(workspacePaths.toolchain, {
    withFileTypes: true,
  })) {
    if (!entry.isDirectory() || !entry.name.startsWith("node-")) continue;
    const executable =
      process.platform === "win32"
        ? path.join(workspacePaths.toolchain, entry.name, "node.exe")
        : path.join(workspacePaths.toolchain, entry.name, "bin", "node");
    if (fs.existsSync(executable)) {
      candidates.push({
        version: entry.name.slice("node-".length),
        executable,
      });
    }
  }

  candidates.sort((a, b) =>
    b.version.localeCompare(a.version, undefined, { numeric: true })
  );
  return candidates[0]?.executable ?? null;
}

export function findLocalPnpm() {
  return fs.existsSync(PNPM_CJS) ? PNPM_CJS : null;
}

export function isInsideRepo(target) {
  const relative = path.relative(repoRoot, path.resolve(target));
  return (
    relative === "" ||
    (!relative.startsWith("..") && !path.isAbsolute(relative))
  );
}

export function isInsideStore(target) {
  const relative = path.relative(
    workspacePaths.pnpmStore,
    path.resolve(target)
  );
  return (
    relative === "" ||
    (!relative.startsWith("..") && !path.isAbsolute(relative))
  );
}

export function ensureWorkspaceDirs() {
  for (const dir of [
    workspacePaths.pnpmStore,
    workspacePaths.pnpmCache,
    workspacePaths.pnpmHome,
    workspacePaths.pnpmState,
    workspacePaths.xdgCache,
    workspacePaths.xdgState,
    workspacePaths.xdgData,
    workspacePaths.xdgConfig,
    workspacePaths.hostTemp,
  ]) {
    fs.mkdirSync(dir, { recursive: true });
  }
}

export function createWorkspaceEnv(baseEnv = process.env) {
  const env = { ...baseEnv };
  const pathKey =
    Object.keys(env).find((key) => key.toUpperCase() === "PATH") ?? "PATH";
  const localNode = findLocalNode();
  const pathEntries = [
    path.join(workspacePaths.toolchain, "pnpm", "node_modules", ".bin"),
    path.join(workspacePaths.cargoHome, "bin"),
    ...(localNode ? [path.dirname(localNode)] : []),
  ];

  env[pathKey] = [...pathEntries, env[pathKey] ?? ""]
    .filter(Boolean)
    .join(path.delimiter);
  env.CARGO_HOME = workspacePaths.cargoHome;
  env.RUSTUP_HOME = workspacePaths.rustupHome;
  env.TEMP = workspacePaths.hostTemp;
  env.TMP = workspacePaths.hostTemp;
  env.TMPDIR = workspacePaths.hostTemp;
  env.PNPM_HOME = workspacePaths.pnpmHome;
  env.npm_config_cache = workspacePaths.pnpmCache;
  env.XDG_CACHE_HOME = workspacePaths.xdgCache;
  env.XDG_STATE_HOME = workspacePaths.xdgState;
  env.XDG_DATA_HOME = workspacePaths.xdgData;
  env.XDG_CONFIG_HOME = workspacePaths.xdgConfig;
  return env;
}
