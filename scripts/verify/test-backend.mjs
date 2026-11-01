import fs from "node:fs";
import path from "node:path";
import http from "node:http";
import net from "node:net";
import { randomUUID } from "node:crypto";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { mihomoLock, verifyMihomo } from "../lib/mihomo-lock.mjs";

const repoRoot = path.resolve(fileURLToPath(new URL("../..", import.meta.url)));
const args = process.argv.slice(2);
const options = new Map();
for (let index = 0; index < args.length; index += 2) {
  const key = args[index];
  if (
    !["--mixed-port", "--upstream-proxy"].includes(key) ||
    !args[index + 1] ||
    options.has(key)
  ) {
    throw new Error(
      "Usage: test:backend [--mixed-port 0|1000..65535] [--upstream-proxy http://host:port]"
    );
  }
  options.set(key, args[index + 1]);
}
let mixedPort = 7897;
if (options.has("--mixed-port")) {
  const value = options.get("--mixed-port");
  if (!/^\d+$/.test(value)) throw new Error("Mixed port must be an integer");
  mixedPort = Number(value);
  if (
    !Number.isInteger(mixedPort) ||
    (mixedPort !== 0 && (mixedPort < 1000 || mixedPort > 65535))
  ) {
    throw new Error("Mixed port must be 0 or between 1000 and 65535");
  }
}
const upstream = options.has("--upstream-proxy")
  ? new URL(options.get("--upstream-proxy"))
  : null;
if (
  upstream &&
  (upstream.protocol !== "http:" ||
    upstream.username ||
    upstream.password ||
    upstream.pathname !== "/" ||
    upstream.search ||
    upstream.hash)
) {
  throw new Error(
    "Test upstream must be an HTTP proxy endpoint without credentials, path, query or fragment"
  );
}
const outbound = upstream ? "TEST_UPSTREAM" : "DIRECT";
if (process.platform !== "win32")
  throw new Error("Backend runtime tests require Windows");
const hostTarget = {
  x64: "x86_64-pc-windows-msvc",
  arm64: "aarch64-pc-windows-msvc",
}[process.arch];
if (!hostTarget)
  throw new Error(`Unsupported test host architecture: ${process.arch}`);
const binary = path.join(
  repoRoot,
  `src-tauri/sidecar/mihomo-${hostTarget}.exe`
);
const workDir = path.join(repoRoot, ".tmp/backend-tests", randomUUID());
fs.mkdirSync(workDir, { recursive: true });
const env = { ...process.env, CARGO_TERM_COLOR: "never" };
const abort = new AbortController();
const children = [];
const reservations = [];
const summary = {
  hostTarget,
  workDir,
  coreBinary: binary,
  outbound,
  upstreamProxy: upstream?.origin ?? null,
  status: "running",
  steps: [],
};
const log = (message) => {
  const line = `${new Date().toISOString()} ${message}\n`;
  fs.appendFileSync(path.join(workDir, "runner.log"), line);
  process.stdout.write(line);
};
const save = () =>
  fs.writeFileSync(
    path.join(workDir, "summary.json"),
    JSON.stringify(summary, null, 2) + "\n"
  );
const interrupt = () => abort.abort(new Error("Backend test run interrupted"));
process.once("SIGINT", interrupt);
process.once("SIGTERM", interrupt);
// 冷构建也受整体时间预算限制，超时只终止本入口创建的命令树。
const deadline = setTimeout(
  () => abort.abort(new Error("Backend test run exceeded 20 minutes")),
  1_200_000
);

async function reserve(port) {
  const server = net.createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port, exclusive: true }, resolve);
  });
  reservations.push(server);
  return server.address().port;
}
async function releaseReservations() {
  await Promise.all(
    reservations
      .splice(0)
      .map(
        (server) =>
          new Promise((resolve, reject) =>
            server.close((error) => (error ? reject(error) : resolve()))
          )
      )
  );
}
function start(name, executable, arguments_, showOutput = false) {
  const started = Date.now();
  const logPath = path.join(workDir, `${name}.log`);
  const child = spawn(executable, arguments_, {
    cwd: repoRoot,
    env,
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const entry = { name, child, closed: false, error: null, output: "" };
  children.push(entry);
  for (const stream of [child.stdout, child.stderr]) {
    stream.on("data", (data) => {
      fs.appendFileSync(logPath, data);
      entry.output += data.toString();
      if (entry.output.length > 8 * 1024 * 1024)
        entry.output = entry.output.slice(-8 * 1024 * 1024);
      if (showOutput) process.stdout.write(data);
    });
  }
  entry.finished = new Promise((resolve) => {
    child.once("error", (error) => {
      entry.error = error;
    });
    child.once("close", (code, signal) => {
      entry.closed = true;
      summary.steps.push({
        name,
        pid: child.pid,
        code,
        signal,
        stoppedByRunner: entry.stoppedByRunner ?? false,
        seconds: (Date.now() - started) / 1000,
      });
      save();
      resolve(code);
    });
  });
  log(`START ${name} pid=${child.pid ?? "unavailable"}`);
  return entry;
}
async function stop(entry) {
  if (entry.closed || !entry.child.pid) return;
  if (entry.child.exitCode !== null) {
    await entry.finished;
    return;
  }
  entry.stoppedByRunner = true;
  // PID 只取自本入口保存的 ChildProcess，禁止按名称终止用户已有内核。
  await new Promise((resolve, reject) => {
    const killer = spawn(
      "taskkill.exe",
      ["/PID", String(entry.child.pid), "/T", "/F"],
      { windowsHide: true, stdio: "ignore" }
    );
    killer.once("error", reject);
    killer.once("close", resolve);
  });
  let timer;
  try {
    await Promise.race([
      entry.finished,
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`Process cleanup timed out: ${entry.name}`)),
          5_000
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}
async function wait(entry, timeout) {
  abort.signal.throwIfAborted();
  let timer;
  let onAbort;
  try {
    const code = await Promise.race([
      entry.finished,
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`${entry.name} timed out`)),
          timeout
        );
      }),
      new Promise((_, reject) => {
        onAbort = () => reject(abort.signal.reason);
        abort.signal.addEventListener("abort", onAbort, { once: true });
      }),
    ]);
    if (entry.error) throw entry.error;
    return code;
  } finally {
    clearTimeout(timer);
    abort.signal.removeEventListener("abort", onAbort);
  }
}
function readConfig(port, secret) {
  return new Promise((resolve, reject) => {
    const request = http.get(
      {
        hostname: "127.0.0.1",
        port,
        path: "/configs",
        headers: { Authorization: `Bearer ${secret}` },
        agent: false,
        timeout: 1000,
      },
      (response) => {
        let body = "";
        response.on("data", (data) => {
          body += data;
        });
        response.once("end", () => {
          if (response.statusCode !== 200)
            return reject(
              new Error(`Controller returned HTTP ${response.statusCode}`)
            );
          try {
            resolve(JSON.parse(body));
          } catch (error) {
            reject(error);
          }
        });
      }
    );
    request.once("timeout", () =>
      request.destroy(new Error("Controller request timed out"))
    );
    request.once("error", reject);
  });
}
async function ready(core, controllerPort, secret) {
  const until = Date.now() + 15_000;
  let lastError;
  while (Date.now() < until) {
    abort.signal.throwIfAborted();
    if (core.closed || core.error)
      throw core.error ?? new Error("Test Mihomo exited before readiness");
    try {
      const config = await readConfig(controllerPort, secret);
      if (config["mixed-port"] !== mixedPort || config.mode !== "rule")
        throw new Error("Unexpected test controller configuration");
      return;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Test Mihomo readiness failed: ${lastError?.message}`);
}

let heartbeat;
try {
  save();
  if (!fs.existsSync(binary))
    throw new Error(`Missing test-host Mihomo binary: ${binary}`);
  if (hostTarget !== mihomoLock.target)
    throw new Error("No pinned core for this host");
  verifyMihomo(binary);
  // Default Tauri custom-protocol tests embed dist at compile time. Build the
  // actual frontend before compiling Rust, including on a clean CI checkout.
  const packageManager = process.env.npm_execpath;
  if (!packageManager || !fs.existsSync(packageManager))
    throw new Error(
      "Run backend tests through the documented pnpm test:backend entrypoint"
    );
  const frontend = start(
    "frontend-build",
    process.execPath,
    [packageManager, "run", "web:build"],
    true
  );
  heartbeat = setInterval(() => {
    log(
      `RUNNING frontend-build pid=${
        frontend.child.pid
      } log_bytes=${Buffer.byteLength(frontend.output)}`
    );
  }, 60_000);
  if ((await wait(frontend, 300_000)) !== 0)
    throw new Error(
      "Frontend build failed before Tauri backend test compilation"
    );
  clearInterval(heartbeat);
  if (!fs.existsSync(path.join(repoRoot, "dist/index.html")))
    throw new Error("Frontend build did not produce dist/index.html");
  log("Frontend assets are ready for Tauri test compilation");
  mixedPort = await reserve(mixedPort);
  const controllerPort = await reserve(0);
  const secret = randomUUID();
  summary.mixedPort = mixedPort;
  summary.controllerPort = controllerPort;
  const proxyDir = path.join(workDir, "proxy");
  fs.mkdirSync(proxyDir);
  const configPath = path.join(proxyDir, "config.yaml");
  // JSON 是 YAML 的合法子集；显式序列化上游端点，避免把参数拼成配置语法。
  fs.writeFileSync(
    configPath,
    JSON.stringify(
      {
        "mixed-port": mixedPort,
        "allow-lan": false,
        "bind-address": "127.0.0.1",
        "external-controller": `127.0.0.1:${controllerPort}`,
        secret,
        mode: "rule",
        "log-level": "info",
        dns: { enable: false },
        proxies: upstream
          ? [
              {
                name: outbound,
                type: "http",
                server: upstream.hostname.startsWith("[")
                  ? upstream.hostname.slice(1, -1)
                  : upstream.hostname,
                port: Number(upstream.port || 80),
              },
            ]
          : [],
        "proxy-groups": [],
        rules: [`MATCH,${outbound}`],
      },
      null,
      2
    ) + "\n"
  );
  env.ASTRA_TEST_MIXED_PORT = String(mixedPort);
  env.ASTRA_TEST_CORE_BINARY = binary;
  env.ASTRA_TEST_REPORT_DIR = path.join(workDir, "integration");
  log(
    `Test proxy uses MATCH,${outbound} on 127.0.0.1:${mixedPort}; upstream=${
      upstream?.origin ?? "none"
    }; no subscription is required`
  );
  const validation = start("proxy-validation", binary, [
    "-t",
    "-d",
    proxyDir,
    "-f",
    configPath,
  ]);
  if ((await wait(validation, 15_000)) !== 0)
    throw new Error("Test proxy configuration validation failed");
  await releaseReservations();
  const core = start("proxy-core", binary, ["-d", proxyDir, "-f", configPath]);
  await ready(core, controllerPort, secret);
  log("Test proxy controller is ready");
  const tests = start(
    "backend-tests",
    "cargo",
    [
      "test",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--locked",
      "--bin",
      "clash-astra",
      "--",
      "--nocapture",
      "--test-threads=1",
    ],
    true
  );
  heartbeat = setInterval(() => {
    log(
      `RUNNING backend-tests cargo_pid=${tests.child.pid} core_pid=${
        core.child.pid
      } test_log_bytes=${Buffer.byteLength(tests.output)}`
    );
    if (core.closed && !tests.closed)
      abort.abort(
        new Error("Test proxy exited while backend tests were running")
      );
  }, 60_000);
  const code = await wait(tests, 1_200_000);
  if (core.closed || core.error) {
    throw (
      core.error ??
      new Error("Test proxy exited before the test suite completed")
    );
  }
  const counts = tests.output.match(
    /test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out/
  );
  summary.tests = counts
    ? {
        result: counts[1],
        passed: Number(counts[2]),
        failed: Number(counts[3]),
        ignored: Number(counts[4]),
        filtered: Number(counts[6]),
      }
    : null;
  summary.probes = [
    ...tests.output.matchAll(
      /connect probe (https:\/\/[^\s:]+): delay=(\d+) ms/g
    ),
  ].map((match) => ({ url: match[1], delayMs: Number(match[2]) }));
  const bothPresent =
    tests.output.includes(
      "test application::diagnostics::tests::connect_probe_through_running_proxy ..."
    ) &&
    tests.output.includes(
      "test infrastructure::mihomo_integration::subscription_compile_reload_and_http_rejection ..."
    );
  if (
    code !== 0 ||
    !counts ||
    summary.tests.passed === 0 ||
    summary.tests.failed !== 0 ||
    summary.tests.ignored !== 0 ||
    summary.tests.filtered !== 0 ||
    !bothPresent
  ) {
    throw new Error(
      `Complete backend tests failed: cargo_exit=${code}; inspect backend-tests.log`
    );
  }
  summary.status = "passed";
} catch (error) {
  summary.status = "failed";
  summary.error = error.message;
  log(`FAILED ${error.message}`);
} finally {
  clearTimeout(deadline);
  clearInterval(heartbeat);
  const cleanupErrors = [];
  for (const child of children.slice().reverse()) {
    try {
      await stop(child);
    } catch (error) {
      cleanupErrors.push(error.message);
    }
  }
  try {
    await releaseReservations();
  } catch (error) {
    cleanupErrors.push(error.message);
  }
  summary.cleanupErrors = cleanupErrors;
  summary.remainingOwnedProcesses = children
    .filter((child) => !child.closed && child.child.pid)
    .map((child) => ({ name: child.name, pid: child.child.pid }));
  if (cleanupErrors.length || summary.remainingOwnedProcesses.length)
    summary.status = "failed";
  save();
  log(`END status=${summary.status} logs=${workDir}`);
  process.removeListener("SIGINT", interrupt);
  process.removeListener("SIGTERM", interrupt);
  process.exitCode = summary.status === "passed" ? 0 : 1;
}
