# scripts

构建、校验与发布脚本。除特别说明外，所有命令都从仓库根目录执行；pnpm 相关命令统一通过 `node scripts/pnpm.mjs <命令>` 进入。

## 入口

| 入口                          | 用途                                                                                          |
| ----------------------------- | --------------------------------------------------------------------------------------------- |
| `pnpm.mjs`                    | 工作区工具链入口：使用本地 Node/pnpm，固定 pnpm store/cache/state、`TEMP` 与 `XDG_*` 到工作区 |
| `check-workspace-hygiene.mjs` | 校验工具链、lockfile、store/cache/state 与 `.gitignore` 约定                                  |
| `release/release-notes.mjs`   | 按 tag 区间生成发布说明，可同步写入 `UPDATELOG.md`                                            |

## 目录

| 目录        | 内容                                                                                     |
| ----------- | ---------------------------------------------------------------------------------------- |
| `build/`    | 构建辅助：`build-watchdog.mjs`、`generate-watchdog-icon.ps1`                             |
| `release/`  | 发布打包：`portable.mjs`；发布说明：`release-notes.mjs`                                  |
| `verify/`   | 校验：`check-naming.mjs`、`test-watchdog.ps1`                                            |
| `sidecars/` | 运行时资源准备：`prepare-runtime.mjs`，下载 mihomo/Geo 数据并构建 Windows service 二进制 |
| `metadata/` | 元数据同步：`sync-metadata.mjs`                                                          |
| `lib/`      | 共享模块：`workspace-env.mjs`                                                            |

## 约定

- package.json 的 scripts 是唯一命令入口；不要在其他文档里另写一套参数。
- 构建产物：`dist/`（前端）、`src-tauri/target/`（Rust）；发布交付物：`artifacts/portable/`；CI staging：`artifacts/staging/`。
- 临时目录一律放 `.tmp/`，任务结束后清理。
