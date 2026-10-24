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

function readNpmrc() {
  const file = path.join(repoRoot, ".npmrc");
  if (!fs.existsSync(file)) return {};
  const result = {};
  for (const raw of fs.readFileSync(file, "utf8").split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#") || line.startsWith(";")) continue;
    const index = line.indexOf("=");
    if (index <= 0) continue;
    result[line.slice(0, index).trim()] = line.slice(index + 1).trim();
  }
  return result;
}

function checkNpmrc() {
  const npmrc = readNpmrc();
  for (const key of ["store-dir", "cache-dir"]) {
    const value = npmrc[key];
    if (!value) {
      fail(`.npmrc ${key}`, "missing");
      continue;
    }
    if (path.isAbsolute(value)) {
      fail(`.npmrc ${key}`, `must stay relative to the repository: ${value}`);
      continue;
    }
    const resolved = path.resolve(repoRoot, value);
    if (!isInsideRepo(resolved)) {
      fail(`.npmrc ${key}`, `resolves outside the repository: ${resolved}`);
      continue;
    }
    pass(`.npmrc ${key}`, resolved);
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

function checkPnpmVersion({ localNode, localPnpm }) {
  if (!localNode || !localPnpm) return;

  const env = createWorkspaceEnv();
  const expected = JSON.parse(
    fs.readFileSync(path.join(repoRoot, "package.json"), "utf8")
  ).packageManager;
  const expectedVersion = String(expected ?? "").replace(/^pnpm@/, "");
  const actual = spawnSync(localNode, [localPnpm, "--version"], {
    cwd: repoRoot,
    env,
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

function checkInstalledModules() {
  const modulesFile = path.join(workspacePaths.nodeModules, ".modules.yaml");
  if (!fs.existsSync(modulesFile)) {
    warn("node_modules", "not installed yet, skipped .modules.yaml check");
    return;
  }

  const storeLine = fs
    .readFileSync(modulesFile, "utf8")
    .split(/\r?\n/)
    .find((line) => line.startsWith("storeDir:"));
  if (!storeLine) {
    fail(".modules.yaml", "storeDir entry not found");
    return;
  }

  const storeDir = storeLine.slice("storeDir:".length).trim();
  if (!isInsideStore(storeDir)) {
    fail(
      ".modules.yaml storeDir",
      `outside ${workspacePaths.pnpmStore}: ${storeDir}`
    );
  } else {
    pass(".modules.yaml storeDir", storeDir);
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
    "pnpm_config_store_dir",
    "pnpm_config_cache_dir",
    "pnpm_config_state_dir",
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
checkNpmrc();
const toolchain = checkToolchain();
checkPnpmVersion(toolchain);
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
