#!/usr/bin/env node
import { spawn } from "node:child_process";
import {
  createWorkspaceEnv,
  ensureWorkspaceDirs,
  findLocalNode,
  findLocalPnpm,
  repoRoot,
  workspacePaths,
} from "./lib/workspace-env.mjs";

const args = process.argv.slice(2);
if (args[0] === "--") args.shift();

const pnpmCjs = findLocalPnpm();
if (!pnpmCjs) {
  console.error(
    `[pnpm] workspace pnpm not found at ${workspacePaths.toolchain}\\pnpm\\node_modules\\pnpm`
  );
  console.error(
    "[pnpm] prepare the workspace toolchain as described in AGENTS.md before running pnpm."
  );
  process.exit(1);
}

ensureWorkspaceDirs();

const localNode = findLocalNode();
if (!localNode) {
  console.warn(
    "[pnpm] .toolchain\\node-*\\node.exe not found, falling back to the current node runtime."
  );
}

const child = spawn(localNode ?? process.execPath, [pnpmCjs, ...args], {
  // Run from the repository root so relative cache-dir settings and script temp dirs resolve there.
  cwd: repoRoot,
  env: createWorkspaceEnv(),
  stdio: "inherit",
});

child.on("error", (error) => {
  console.error(`[pnpm] failed to start pnpm: ${error.message}`);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
