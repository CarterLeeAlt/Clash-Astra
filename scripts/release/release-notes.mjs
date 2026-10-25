#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { repoRoot } from "../lib/workspace-env.mjs";

// 发布说明生成：按「上一个 tag .. 本次 ref」的提交生成中文分段说明，
// 同时可按 Added/Changed/Fixed 写入 UPDATELOG.md。

const SECTION_ORDER = [
  "新功能",
  "修复",
  "优化",
  "构建与发布",
  "文档",
  "测试",
  "其他",
];
const SECTION_BY_TYPE = {
  feat: "新功能",
  fix: "修复",
  perf: "优化",
  refactor: "优化",
  style: "优化",
  build: "构建与发布",
  ci: "构建与发布",
  chore: "构建与发布",
  docs: "文档",
  test: "测试",
};
const UPDATELOG_ORDER = ["Added", "Changed", "Fixed"];
const UPDATELOG_BY_TYPE = { feat: "Added", fix: "Fixed" };

// 版本提交与合并提交不进入发布说明
const SKIPPED_SUBJECTS = [
  /^chore\(release\)/i,
  /^chore:\s*(release|bump|version)/i,
  /^Merge\b/,
];

const FIELD_SEPARATOR = "\x1f";
const RECORD_SEPARATOR = "\x1e";

function git(args) {
  return execFileSync("git", ["-c", "core.quotepath=false", ...args], {
    cwd: repoRoot,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function log(message) {
  process.stderr.write(`[release-notes] ${message}\n`);
}

function fail(message) {
  log(message);
  process.exit(1);
}

function parseArgs(argv) {
  const options = {
    version: null,
    from: null,
    to: "HEAD",
    out: null,
    updatelog: "UPDATELOG.md",
    writeUpdatelog: false,
    notesDir: "docs/release/notes",
    withHashes: false,
    help: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = () => {
      const value = argv[index + 1];
      if (!value || value.startsWith("--")) fail(`缺少参数值：${arg}`);
      index += 1;
      return value;
    };

    switch (arg) {
      case "--version":
        options.version = next();
        break;
      case "--from":
        options.from = next();
        break;
      case "--to":
        options.to = next();
        break;
      case "--out":
        options.out = next();
        break;
      case "--updatelog":
        options.updatelog = next();
        break;
      case "--write-updatelog":
        options.writeUpdatelog = true;
        break;
      case "--notes-dir":
        options.notesDir = next();
        break;
      case "--with-hashes":
        options.withHashes = true;
        break;
      case "-h":
      case "--help":
        options.help = true;
        break;
      default:
        fail(`未知参数：${arg}`);
    }
  }

  return options;
}

function usage() {
  return [
    "用法：node scripts/release/release-notes.mjs [选项]",
    "",
    "  --version <x.y.z>    目标版本，缺省读 package.json",
    "  --from <tag>         起始 tag（不含），缺省自动取小于目标版本、且已并入 --to 的最大 tag",
    "  --to <ref>           结束 ref，默认 HEAD",
    "  --out <file>         写入发布说明文件，缺省输出到 stdout",
    "  --write-updatelog    把说明写入 UPDATELOG.md 顶部对应版本小节",
    "  --updatelog <file>   UPDATELOG 路径，默认 UPDATELOG.md",
    "  --notes-dir <dir>    人工说明目录，默认 docs/release/notes",
    "  --with-hashes        每条附提交短哈希链接",
    "",
    "人工覆盖：docs/release/notes/v<版本>.md 存在时优先作为发布说明正文。",
  ].join("\n");
}

function readPackageJson() {
  const file = path.join(repoRoot, "package.json");
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function compareVersions(left, right) {
  const leftParts = left
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0);
  const rightParts = right
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0);
  const length = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < length; index += 1) {
    const diff = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

function resolveFromTag(version, to) {
  const tags = git(["tag", "--list", "v*", "--merged", to, "--sort=-v:refname"])
    .split(/\r?\n/)
    .map((tag) => tag.trim())
    .filter(Boolean);

  for (const tag of tags) {
    if (compareVersions(tag.replace(/^v/, ""), version) < 0) return tag;
  }
  return null;
}

function readCommits(range) {
  const format = `${["%H", "%h", "%s", "%b"].join("%x1f")}%x1e`;
  let raw = "";
  try {
    raw = git(["log", "--no-merges", `--format=${format}`, range]);
  } catch (error) {
    fail(
      `读取提交失败（${range}）：${String(
        error.stderr ?? error.message
      ).trim()}`
    );
  }
  return raw
    .split(RECORD_SEPARATOR)
    .map((record) => record.replace(/^\r?\n/, "").trimEnd())
    .filter(Boolean)
    .map((record) => {
      const [hash, shortHash, subject, body = ""] =
        record.split(FIELD_SEPARATOR);
      return { hash, shortHash, subject, body };
    });
}

function stripNoise(text) {
  return text
    .replace(/\[skip ci\]/gi, "")
    .replace(/\s+/g, " ")
    .trim();
}

function parseCommit(commit) {
  const subject = stripNoise(commit.subject);
  const match = /^([a-zA-Z]+)(?:\([^)]*\))?!?:\s*(.+)$/.exec(subject);
  const type = match ? match[1].toLowerCase() : "other";
  const title = match ? stripNoise(match[2]) : subject;

  const details = commit.body
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter((line) => /^[-*]\s+\S/.test(line))
    .map((line) => `- ${stripNoise(line.replace(/^[-*]\s+/, ""))}`)
    .filter((line) => line !== "-")
    .filter((line) => !/^-\s*(Signed-off-by|Co-authored-by):/i.test(line));

  return { ...commit, type, title, details };
}

function shouldSkip(commit) {
  return SKIPPED_SUBJECTS.some((pattern) =>
    pattern.test(stripNoise(commit.subject))
  );
}

function collectEntries(range, withHashes, repoUrl) {
  return readCommits(range)
    .filter((commit) => !shouldSkip(commit))
    .map(parseCommit)
    .map((commit) => ({
      ...commit,
      link: withHashes ? `${repoUrl}/commit/${commit.hash}` : null,
    }));
}

function groupEntries(entries, sectionOf) {
  const groups = new Map();
  for (const entry of entries) {
    const section = sectionOf(entry.type);
    if (!groups.has(section)) groups.set(section, []);
    groups.get(section).push(entry);
  }
  return groups;
}

function renderRelease({ entries, version, from, repoUrl }) {
  const groups = groupEntries(
    entries,
    (type) => SECTION_BY_TYPE[type] ?? "其他"
  );
  const blocks = [];

  for (const section of SECTION_ORDER) {
    const items = groups.get(section);
    if (!items || items.length === 0) continue;

    const lines = [`## ${section}`, ""];
    for (const item of items) {
      lines.push(
        item.link
          ? `- ${item.title}（[${item.shortHash}](${item.link})）`
          : `- ${item.title}`
      );
      for (const detail of item.details) lines.push(`  ${detail}`);
    }
    blocks.push(lines.join("\n"));
  }

  if (blocks.length === 0) {
    blocks.push("本版本没有面向用户的变更。");
  }

  if (from) {
    const range = `${from}...v${version}`;
    blocks.push(`---\n\n**完整变更**：[${range}](${repoUrl}/compare/${range})`);
  }

  return `${blocks.join("\n\n")}\n`;
}

function renderUpdatelog({ entries, version }) {
  const groups = groupEntries(
    entries,
    (type) => UPDATELOG_BY_TYPE[type] ?? "Changed"
  );
  const lines = [`## ${version}`, ""];

  for (const section of UPDATELOG_ORDER) {
    const items = groups.get(section);
    if (!items || items.length === 0) continue;
    lines.push(`### ${section}`, "");
    for (const item of items) lines.push(`- ${item.title}`);
    lines.push("");
  }

  if (lines.length === 2) lines.push("- 仅包含版本与构建变更。", "");
  return lines.join("\n").trimEnd();
}

function splitUpdatelog(text) {
  const head = [];
  const sections = [];
  let current = null;

  for (const line of text.split(/\r?\n/)) {
    if (/^##\s+/.test(line)) {
      if (current) sections.push(current);
      current = [line];
    } else if (current) {
      current.push(line);
    } else {
      head.push(line);
    }
  }
  if (current) sections.push(current);

  return {
    head: head.join("\n").trimEnd(),
    sections: sections.map((lines) => lines.join("\n").trimEnd()),
  };
}

function writeUpdatelogSection(target, version, sectionText) {
  const file = path.resolve(repoRoot, target);
  const source = fs.existsSync(file)
    ? fs.readFileSync(file, "utf8")
    : "# Update Log";
  const { head, sections } = splitUpdatelog(source);
  const headerPattern = new RegExp(
    `^##\\s+v?${version.replace(/\./g, "\\.")}\\s*$`
  );

  const kept = sections.filter(
    (section) => !headerPattern.test(section.split(/\r?\n/)[0])
  );
  const replaced = kept.length !== sections.length;
  const contents = [head, sectionText, ...kept].filter(Boolean).join("\n\n");

  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${contents}\n`, "utf8");
  log(
    `${replaced ? "更新" : "写入"} UPDATELOG 小节：${path.relative(
      repoRoot,
      file
    )} → ${version}`
  );
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(`${usage()}\n`);
    return;
  }

  const packageJson = readPackageJson();
  const version = options.version ?? packageJson.version;
  if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
    fail(`版本号无效：${version}`);
  }

  const repoUrl = (packageJson.homepage ?? "").replace(/\/$/, "");
  if (!repoUrl) fail("package.json 缺少 homepage，无法生成 compare 链接");

  const from = options.from ?? resolveFromTag(version, options.to);
  if (!from) log("未找到上一个 tag，将输出全部历史提交的说明");

  const range = from ? `${from}..${options.to}` : options.to;
  const entries = collectEntries(range, options.withHashes, repoUrl);
  log(
    `区间 ${from ?? "(起点)"}..${options.to}，用户可见提交 ${entries.length} 条`
  );

  const manualFile = path.resolve(repoRoot, options.notesDir, `v${version}.md`);
  const manualNotes = fs.existsSync(manualFile)
    ? fs.readFileSync(manualFile, "utf8").trimEnd()
    : null;
  if (manualNotes) log(`使用人工说明：${path.relative(repoRoot, manualFile)}`);

  const body =
    manualNotes ?? renderRelease({ entries, version, from, repoUrl });

  if (options.writeUpdatelog) {
    writeUpdatelogSection(
      options.updatelog,
      version,
      renderUpdatelog({ entries, version })
    );
  }

  if (options.out) {
    const outFile = path.resolve(repoRoot, options.out);
    fs.mkdirSync(path.dirname(outFile), { recursive: true });
    fs.writeFileSync(outFile, body.endsWith("\n") ? body : `${body}\n`, "utf8");
    log(`发布说明写入：${path.relative(repoRoot, outFile)}`);
    return;
  }

  process.stdout.write(body.endsWith("\n") ? body : `${body}\n`);
}

main();
