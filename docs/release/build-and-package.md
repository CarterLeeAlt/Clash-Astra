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

内核由 `src-tauri/mihomo.lock.json` 固定为 `v1.19.33`，不查询 `latest`。存在的内核也必须匹配版本和 SHA-256，否则失败；`--force` 不能绕过身份检查。构建前的 `check-mihomo.mjs`、打包和运行前均核对同一文件。版本、常用字段和三方联合验收见 [适配契约](../reference/mihomo-contract.md)。

## 构建

```powershell
node scripts/pnpm.mjs run build
```

该命令依次执行：

1. `tauri build --bundles none`，其中 `beforeBuildCommand` 会运行 `web:build`（`tsc` + `vite build`）；主程序按 `productName` 产出 `Clash Astra.exe`。
2. `scripts/build/build-watchdog.mjs` 构建 Watchdog 并放置为 `Clash Astra Watchdog.exe`。

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

# 完整后端测试，包含当前代理外网探测和独立真实内核集成
cargo test --manifest-path src-tauri/Cargo.toml --locked -- --nocapture

# 本机与 CI 共用的隔离环境入口；自动启动无订阅 Mihomo 再运行全部后端测试
node scripts/pnpm.mjs run test:backend

# 本机已有 7897 代理、直连外网受限时，显式选择临时端口和上游
node scripts/pnpm.mjs run test:backend --mixed-port 0 --upstream-proxy http://127.0.0.1:20081
```

直接运行 `cargo test` 时，后端测试默认实际执行，不跳过环境依赖项；须确保本机 `127.0.0.1:7897` 有可用的 Mihomo 混合代理，并已准备工作区 `src-tauri/sidecar/mihomo-x86_64-pc-windows-msvc.exe`。外网探测会通过运行实例的当前路由访问 Google、YouTube、ChatGPT 和 GitHub，仅测到 TLS 首字节响应，不完成网页请求，也不修改节点或系统代理。每站连接探测上限 10 秒；四站全部记录后统一断言，任一失败会使测试失败。使用 `test:backend` 入口时由脚本准备独立代理，无需用户提前启动代理。

独立内核集成默认使用 `src-tauri/tests/fixtures/mihomo-control.yaml`，无需设置环境变量；该无凭据夹具只用于控制接口、配置重载和事务回滚验证，不代表远程节点可用。需要其他输入时，可在进程级设置 `ASTRA_TEST_SUBSCRIPTION_FILE` 指向工作区内的测试配置。缺少内核、代理或指定输入，或网络与断言不满足时，必须报告失败，不能视为跳过或通过。

`test:backend` 先运行 `web:build` 生成真实前端产物，再编译和执行默认 Tauri 后端测试；`custom-protocol` 编译需要 `dist/index.html`，干净工作区无需预留旧产物或空占位目录。前端构建失败直接阻止后端测试，步骤日志与结果一并记录。

该入口默认使用无订阅的 `MATCH,DIRECT` 配置，监听 `7897`，控制器另选临时端口；已有进程占用目标端口时明确失败，不接管或终止它。`--mixed-port 0` 由系统选择临时空闲端口，`--upstream-proxy` 只接受无认证信息的 HTTP 代理端点。端口、宿主架构内核路径和报告目录由入口显式传入同一套测试，不添加测试过滤或忽略开关。

发布 CI 调用 `pnpm test:backend`，自动准备上述无订阅运行环境，仍使用全部测试与相同断言；不传本机上游参数，四站探测使用 GitHub runner 自身网络。Mihomo 配置校验、控制器就绪、测试、超时和收尾均由入口负责。单轮整体预算 20 分钟、就绪上限 15 秒；定期输出运行状态，失败或正常完成均清理自建进程。工作流另设 25 分钟步骤预算，并无论测试成败归档 `.tmp/backend-tests/` 中的日志和汇总 JSON，保留 7 天，不上传配置或用户数据。

本机与 CI 的测试用例和断言相同，但网络出口不同；真实外网波动或 runner 出口不可达会导致 CI 失败，不能通过过滤、忽略或自动回退规避。本机显式上游验证通过不代表 GitHub DIRECT 出口已经验证，真实订阅和节点质量另在本机验证。

系统代理依赖库的默认完整测试：

```powershell
node scripts/pnpm.mjs exec cargo test --manifest-path src-tauri/Cargo.toml -p sysproxy --locked
```

该入口只运行只读检查和生产逻辑的纯单元测试，不包含真实系统代理写入目标，也不要求额外宿主环境。禁用状态按空地址和端口 `0` 的接口语义断言；生产恢复字段核验、有限重试与持续查询失败通过内存输入验证。它们不代表真实 Windows 写入恢复已经实测，实际网络变更另行获准并留存操作前状态，见 [sysproxy 测试说明](../../src-tauri/vendor-crates/sysproxy-rs/README.md)。

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
- `PORTABLE_LABEL`：可选的本地修复包标签，如 `icons-fixed`；只接受小写字母、数字和单个连字符，标签位于 `portable` 与 `unsigned` 之间，用于保留同版本的旧包。

输出：

```text
artifacts/portable/clash-astra_<version>_x64_portable_unsigned.zip
```

未签名时带 `_unsigned`，版本取自 `package.json`。zip 根目录包含 `Clash Astra.exe`、`Clash Astra Watchdog.exe`、唯一的 `mihomo.exe` 和 `resources/`。Mihomo 的版本与 SHA-256 仍由源码中的 `src-tauri/mihomo.lock.json` 在构建前、打包前和运行前校验。

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
