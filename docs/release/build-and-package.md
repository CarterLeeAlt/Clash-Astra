# 构建、检查与打包

本文描述 Windows 本机的构建、检查和便携包流程。目录职责见 [仓库目录与产物边界](../architecture/repository-layout.md)，Watchdog 行为见 [Watchdog 设计与行为](../architecture/watchdog.md)。

## 工具链

- 使用工作区内工具链：`.toolchain/`（Node、pnpm）、`.cargo/`、`.rustup/`；不要把依赖或缓存装到全局路径。
- Node、pnpm、Rust 的具体版本以 `package.json`、`AGENTS.md` 和 lockfile 为准，不在本文重复维护。
- 所有 pnpm 命令从仓库根通过入口执行：

```powershell
node scripts/pnpm.mjs <命令>
```

该入口会前置本地 Node/pnpm/Rust，并把 `TEMP`、pnpm store/cache/state、`XDG_*` 固定在工作区。

## 安装依赖与运行时资源

```powershell
# 安装前端依赖
node scripts/pnpm.mjs install

# 首次构建或资源缺失时：下载 mihomo/Geo 数据并构建 Windows service 二进制
node scripts/pnpm.mjs run prepare:runtime
```

`prepare:runtime` 不是代码检查；已有资源会被复用，只有缺失或传 `--force` 时才重新下载。

## 构建

```powershell
node scripts/pnpm.mjs run build
```

该命令依次执行：

1. `tauri build --bundles none`，其中 `beforeBuildCommand` 会运行 `web:build`（`tsc` + `vite build`）。
2. `scripts/build/build-watchdog.mjs` 构建并放置 Watchdog。
3. `scripts/build/normalize-main-exe.mjs` 把 Tauri 主程序改名为 `clash-astra.exe`。

只构建 Watchdog 时：

```powershell
node scripts/pnpm.mjs run build:watchdog
```

## 检查与测试

```powershell
# 前端类型检查
node scripts/pnpm.mjs exec tsc --noEmit

# 命名与版本一致性
node scripts/pnpm.mjs run check:naming

# 工具链、lockfile、store/cache/state 位置
node scripts/pnpm.mjs run check:workspace
```

Rust 检查按 `AGENTS.md` 设置工作区环境后执行：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
```

Watchdog 黑盒回归：

```powershell
pwsh -File scripts/verify/test-watchdog.ps1
```

脚本需要本地 `rustc` 可执行，并写入 `.tmp/` 下的独立测试目录。

## 打包

```powershell
$env:SKIP_RELEASE_UPLOAD = "1"
$env:UNSIGNED_BUILD = "1"
node scripts/pnpm.mjs run portable x86_64-pc-windows-msvc
```

- `SKIP_RELEASE_UPLOAD=1`：只生成 zip，不访问 GitHub Release。
- `UNSIGNED_BUILD=1`：未签名构建，文件名带 `_unsigned`；签名模式见 [Windows 签名](windows-signing.md)。

输出：

```text
artifacts/portable/clash-astra_<version>_x64_portable_unsigned.zip
```

未签名时带 `_unsigned`，版本取自 `package.json`。zip 根目录包含 GUI、Watchdog、`mihomo.exe`、`mihomo-alpha.exe` 和 `resources/`。

## 产物边界

| 路径                                         | 说明                                        |
| -------------------------------------------- | ------------------------------------------- |
| `dist/`                                      | Vite 前端产物；每次 `vite build` 会整体清空 |
| `src-tauri/target/`                          | Rust 构建输出                               |
| `src-tauri/sidecar/`、`src-tauri/resources/` | 构建输入，由 `prepare:runtime` 准备         |
| `artifacts/portable/`                        | 便携包，`vite build` 不会清理               |
| `artifacts/staging/`                         | CI 发布 staging                             |

打包后可以再运行一次 `node scripts/pnpm.mjs run build` 验证：`dist/` 会被重建，但 `artifacts/portable/` 中的 zip 应保持不变。

## CI

GitHub Actions 使用 `pnpm/action-setup` 安装的 pnpm，不使用本机 wrapper；构建步骤与本机一致。release 的 staging 在 `artifacts/staging/` 下，上传路径由工作流维护。工作流 checkout 使用 `fetch-depth: 0`：生成发布说明要读取上一个 tag 与完整提交历史。

## 发布说明

Release 页面的说明由 `scripts/release/release-notes.mjs` 按「上一个 tag .. 本次 tag」的提交生成，不使用 GitHub 的 `generate_release_notes`（它只统计已合并的 PR，本仓库直接提交到 `main`，结果只有 compare 链接）。

分组规则：

- `feat` → 新功能；`fix` → 修复；`perf`、`refactor`、`style` → 优化；`build`、`ci`、`chore` → 构建与发布；`docs` → 文档；`test` → 测试。
- 提交标题作为条目，正文中以 `-` 或 `*` 开头的行作为子条。
- `chore(release):` 版本提交与合并提交不收录。

自动发版（push 到 `main`）时，同一份内容还会以 `Added`、`Changed`、`Fixed` 小节写进 `UPDATELOG.md` 顶部，随版本提交一起推送。手动重发已存在的 tag 时不改 `UPDATELOG.md`。

本地预览：

```powershell
node scripts/release/release-notes.mjs --version 1.8.1 --from v1.7.2
```

常用选项：`--from`、`--to` 指定区间（缺省自动取小于目标版本且已并入 `--to` 的最大 tag），`--out` 写入文件，`--write-updatelog` 同步 `UPDATELOG.md`，`--with-hashes` 附提交链接。

人工覆盖：`docs/release/notes/v<版本>.md` 存在时，其内容直接作为 Release 正文（`UPDATELOG.md` 仍按提交生成）。CI 中说明文件写在 `artifacts/staging/notes.md`，`artifacts/staging/release/` 由资产校验步骤整体重建，不要放进该目录。

## 验收清单

- `node scripts/pnpm.mjs exec tsc --noEmit` 通过。
- `node scripts/pnpm.mjs run check:naming`、`run check:workspace` 通过。
- `cargo fmt --check`、`cargo check` 通过。
- Watchdog 六项黑盒回归通过。
- `artifacts/portable/` 中的 zip 存在，且再次构建后仍存在。
- 构建与打包期间没有在工作区外产生缓存、store 或临时文件。
