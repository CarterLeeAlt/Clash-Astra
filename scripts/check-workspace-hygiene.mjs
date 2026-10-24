#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import {
  createWorkspaceEnv,
  ensureWorkspaceDirs,
  findLocalNode,
  findLocalPnpm,
  isInsideRepo,
  isInsideStore,
  repoRoot,
  workspacePaths,
} from "./lib/workspace-env.mjs";

const failures = [];
const warnings = [];

ensureWorkspaceDirs();

function pass(label, detail) {
  console.log(`[ok]   ${label}${detail ? `: ${detail}` : ""}`);
}

function fail(label, detail) {
  failures.push(label);
  console.error(`[fail] ${label}${detail ? `: ${detail}` : ""}`);
}

function warn(label, detail) {
  warnings.push(label);
  console.warn(`[warn] ${label}${detail ? `: ${detail}` : ""}`);
}

function parseVersion(value) {
  return String(value)
    .replace(/^v/, "")
    .split(/[^0-9]+/)
    .filter(Boolean)
    .map((part) => Number.parseInt(part, 10));
}

function versionAtLeast(actual, expected) {
  const left = parseVersion(actual);
  const right = parseVersion(expected);
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const diff = (left[index] ?? 0) - (right[index] ?? 0);
    if (diff !== 0) return diff > 0;
  }
  return true;
}

function readSettings(file) {
  const result = {};
  if (!fs.existsSync(file)) return result;
  for (const raw of fs.readFileSync(file, "utf8").split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#") || line.startsWith(";")) continue;
    const match = line.match(/^([^:=\s][^:=]*?)\s*[:=]\s*(.*)$/);
    if (!match) continue;
    const value = match[2].trim().replace(/^['"]|['"]$/g, "");
    if (value) result[match[1].trim()] = value;
  }
  return result;
}

function checkWorkspaceConfig() {
  const file = path.join(repoRoot, "pnpm-workspace.yaml");
  if (!fs.existsSync(file)) {
    fail("pnpm-workspace.yaml", "missing");
    return;
  }

  const config = readSettings(file);
  for (const key of ["storeDir", "cacheDir", "stateDir"]) {
    const value = config[key];
    if (!value) {
      fail(`pnpm-workspace.yaml ${key}`, "missing");
      continue;
    }
    if (path.isAbsolute(value)) {
      fail(`pnpm-workspace.yaml ${key}`, `must stay relative: ${value}`);
      continue;
    }
    const resolved = path.resolve(repoRoot, value);
    if (!isInsideRepo(resolved)) {
      fail(`pnpm-workspace.yaml ${key}`, `outside the repository: ${resolved}`);
      continue;
    }
    pass(`pnpm-workspace.yaml ${key}`, resolved);
  }
}

function checkBuildPolicy() {
  const file = path.join(repoRoot, "pnpm-workspace.yaml");
  if (!fs.existsSync(file)) return;
  const text = fs.readFileSync(file, "utf8");

  if (/^onlyBuiltDependencies:/m.test(text)) {
    fail(
      "pnpm-workspace.yaml onlyBuiltDependencies",
      "ignored by pnpm 11; use allowBuilds"
    );
  } else {
    pass("pnpm-workspace.yaml onlyBuiltDependencies", "not set (expected)");
  }

  if (!/^\s+esbuild:\s*true\s*$/m.test(text)) {
    fail("pnpm-workspace.yaml allowBuilds", "missing esbuild: true");
  } else {
    pass("pnpm-workspace.yaml allowBuilds", "esbuild: true");
  }
}

function checkNpmrc() {
  const npmrc = readSettings(path.join(repoRoot, ".npmrc"));
  for (const key of ["store-dir", "cache-dir", "state-dir"]) {
    if (npmrc[key]) {
      fail(
        `.npmrc ${key}`,
        "pnpm 11 ignores it; move the setting to pnpm-workspace.yaml"
      );
    } else {
      pass(`.npmrc ${key}`, "not set (expected)");
    }
  }
}

function checkToolchain() {
  const localNode = findLocalNode();
  if (localNode) {
    pass("local node", localNode);
  } else {
    fail("local node", "missing .toolchain\\node-*\\node.exe");
  }

  const localPnpm = findLocalPnpm();
  if (localPnpm) {
    pass("local pnpm", localPnpm);
  } else {
    fail("local pnpm", "missing .toolchain\\pnpm\\node_modules\\pnpm");
  }

  return { localNode, localPnpm };
}

function checkNodeVersion(localNode) {
  if (!localNode) return;
  const result = spawnSync(localNode, ["--version"], { encoding: "utf8" });
  if (result.status !== 0) {
    fail("node --version", result.stderr.trim() || "command failed");
    return;
  }
  const version = result.stdout.trim();
  if (!versionAtLeast(version, "22.13.0")) {
    fail("node version", `${version} is older than 22.13.0`);
  } else {
    pass("node version", version);
  }
}

function checkPnpmVersion({ localNode, localPnpm }) {
  if (!localNode || !localPnpm) return;

  const expected = JSON.parse(
    fs.readFileSync(path.join(repoRoot, "package.json"), "utf8")
  ).packageManager;
  const expectedVersion = String(expected ?? "").replace(/^pnpm@/, "");
  const actual = spawnSync(localNode, [localPnpm, "--version"], {
    cwd: repoRoot,
    env: createWorkspaceEnv(),
    encoding: "utf8",
  });

  if (actual.status !== 0) {
    fail("local pnpm --version", actual.stderr.trim() || "command failed");
    return;
  }

  const version = actual.stdout.trim();
  if (expectedVersion && version !== expectedVersion) {
    fail(
      "pnpm version",
      `workspace pnpm is ${version}, package.json pins ${expectedVersion}`
    );
  } else {
    pass("pnpm version", version);
  }
}

function checkLockfile() {
  const file = path.join(repoRoot, "pnpm-lock.yaml");
  if (!fs.existsSync(file)) {
    fail("pnpm-lock.yaml", "missing");
    return;
  }
  const firstLine = fs.readFileSync(file, "utf8").split(/\r?\n/)[0].trim();
  if (!/^lockfileVersion:\s*['"]?9\.0['"]?$/.test(firstLine)) {
    fail("pnpm-lock.yaml", `expected lockfileVersion 9.0, got ${firstLine}`);
  } else {
    pass("pnpm-lock.yaml", firstLine);
  }
}

function checkStorePath({ localNode, localPnpm }) {
  if (!localNode || !localPnpm) return;

  const result = spawnSync(localNode, [localPnpm, "store", "path"], {
    cwd: repoRoot,
    env: createWorkspaceEnv(),
    encoding: "utf8",
  });

  if (result.status !== 0) {
    fail("pnpm store path", result.stderr.trim() || "command failed");
    return;
  }

  const storePath = result.stdout.trim();
  if (!isInsideStore(storePath)) {
    fail(
      "pnpm store path",
      `outside ${workspacePaths.pnpmStore}: ${storePath}`
    );
  } else {
    pass("pnpm store path", storePath);
  }
}

function checkWrapperEnv() {
  const env = createWorkspaceEnv();
  const keys = [
    "TEMP",
    "TMP",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "PNPM_HOME",
    "npm_config_cache",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "XDG_DATA_HOME",
    "XDG_CONFIG_HOME",
  ];
  const outside = keys.filter((key) => !isInsideRepo(env[key]));
  if (outside.length > 0) {
    for (const key of outside) {
      fail(`wrapper env ${key}`, `outside the repository: ${env[key]}`);
    }
    return;
  }
  pass("wrapper env", "TEMP and toolchain dirs stay in the repository");
}

function checkInstalledModules() {
  const modulesFile = path.join(workspacePaths.nodeModules, ".modules.yaml");
  if (!fs.existsSync(modulesFile)) {
    warn("node_modules", "not installed yet, skipped .modules.yaml check");
    return;
  }

  const modulesText = fs.readFileSync(modulesFile, "utf8");
  const storeMatch = modulesText.match(
    /^\s*"?storeDir"?\s*:\s*(.+?)\s*,?\s*$/m
  );
  if (!storeMatch) {
    fail(".modules.yaml", "storeDir entry not found");
    return;
  }

  const storeDir = storeMatch[1].replace(/^['"]|['"]$/g, "");
  if (!isInsideStore(storeDir)) {
    fail(
      ".modules.yaml storeDir",
      `outside ${workspacePaths.pnpmStore}: ${storeDir}`
    );
  } else {
    pass(".modules.yaml storeDir", storeDir);
  }
}

function checkGitignore() {
  const file = path.join(repoRoot, ".gitignore");
  const lines = fs.existsSync(file)
    ? fs.readFileSync(file, "utf8").split(/\r?\n/)
    : [];
  for (const entry of ["/.toolchain/", "/.tmp/"]) {
    if (lines.includes(entry)) {
      pass(".gitignore", entry);
    } else {
      fail(".gitignore", `missing ${entry}`);
    }
  }
}

function checkDriveRootStore() {
  const driveRoot = path.parse(repoRoot).root;
  const driveStore = path.join(driveRoot, ".pnpm-store");
  if (fs.existsSync(driveStore)) {
    warn(
      "drive-root pnpm store",
      `${driveStore} exists; this project never uses it, remove it manually if it belongs to no other project`
    );
  }
}

console.log(`[workspace] ${repoRoot}`);
checkWorkspaceConfig();
checkBuildPolicy();
checkNpmrc();
const toolchain = checkToolchain();
checkNodeVersion(toolchain.localNode);
checkPnpmVersion(toolchain);
checkLockfile();
checkStorePath(toolchain);
checkWrapperEnv();
checkInstalledModules();
checkGitignore();
checkDriveRootStore();

if (failures.length > 0) {
  console.error(
    `\n[workspace] ${failures.length} check(s) failed. pnpm commands must run through "node scripts/pnpm.mjs".`
  );
  process.exit(1);
}

console.log(
  `\n[workspace] all checks passed${
    warnings.length > 0 ? ` (${warnings.length} warning(s))` : ""
  }.`
);
