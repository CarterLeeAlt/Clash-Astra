import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";

// 文档和报告不参与源码指纹，避免在验收报告中记录指纹时产生自引用。
export function sourceIdentity(root) {
  const git = (args) =>
    execFileSync("git", args, {
      cwd: root,
      encoding: "utf8",
      windowsHide: true,
    }).trim();
  const names = [
    ...new Set([
      ...git(["ls-files", "-z"]).split("\0"),
      ...git([
        "ls-files",
        "-z",
        "--others",
        "--exclude-standard",
        "--",
        "src",
        "src-tauri",
        "scripts",
        "tests",
        "test",
        ".github",
      ]).split("\0"),
    ]),
  ]
    .filter(
      (name) =>
        name &&
        !name.startsWith("docs/") &&
        !/\.md$/.test(name) &&
        fs.existsSync(path.join(root, name))
    )
    .sort();
  const hash = createHash("sha256");
  for (const name of names) {
    const bytes = fs.readFileSync(path.join(root, name));
    hash.update(name);
    hash.update("\0");
    hash.update(
      /\.(rs|js|jsx|mjs|ts|tsx|json|yaml|yml|toml|lock|ps1|html|css|scss|svg)$/.test(
        name
      )
        ? Buffer.from(bytes.toString("utf8").replaceAll("\r\n", "\n"))
        : bytes
    );
    hash.update("\0");
  }
  return {
    baseCommit: git(["rev-parse", "HEAD"]),
    modified:
      !!git(["diff", "HEAD", "--name-only"]) ||
      !!git([
        "ls-files",
        "--others",
        "--exclude-standard",
        "--",
        "src",
        "src-tauri",
        "scripts",
        "tests",
        "test",
      ]),
    sha256: hash.digest("hex"),
    scope:
      "tracked and nonignored new source files; excludes Markdown/docs; text CRLF normalized",
  };
}
