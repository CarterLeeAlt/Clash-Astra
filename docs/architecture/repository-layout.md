# 仓库目录与产物边界

## 目标

- 源码、构建输出、发布交付物、临时文件分开，避免构建清理误删交付物。
- Tauri 要求的路径保持原位；移动前必须同步配置、脚本与 CI。
- 当前操作说明与历史验证记录分开，过期内容不再混在现行文档里。

## 目录

```text
Clash-Astra/
├─ src/                          # React 前端源码
├─ src-tauri/                    # Tauri/Rust 主程序
│  ├─ src/
│  ├─ watchdog/                  # 独立 watchdog crate
│  ├─ windows-service-src/       # Windows service crate
│  ├─ vendor-crates/             # 内部 vendored crate
│  ├─ icons/ sidecar/ resources/ local-binaries/
│  └─ target/                    # Rust 构建输出（忽略）
├─ scripts/                      # 构建、校验、发布脚本（见 scripts/README.md）
├─ docs/                         # 当前文档与历史归档（见 docs/README.md）
├─ artifacts/                    # 发布交付物与 CI staging（忽略）
├─ dist/                         # Vite 前端产物（忽略）
├─ .github/ .husky/
├─ .toolchain/ .cargo/ .rustup/ node_modules/ .tmp/   # 本地环境与临时目录（忽略）
├─ package.json pnpm-lock.yaml pnpm-workspace.yaml
├─ README.md CONTRIBUTING.md AGENTS.md UPDATELOG.md
```

## 职责

| 路径                             | 类型      | 用途                                                     |
| -------------------------------- | --------- | -------------------------------------------------------- |
| `src/`                           | 源码      | 前端页面、组件、状态与样式                               |
| `src-tauri/src/`                 | 源码      | Tauri 主程序                                             |
| `src-tauri/watchdog/`            | 源码      | 独立 Watchdog crate                                      |
| `src-tauri/windows-service-src/` | 源码      | Windows service 与 helper                                |
| `src-tauri/vendor-crates/`       | 源码      | 内部 vendored crate                                      |
| `src-tauri/icons/`               | 源码/资源 | Tauri 图标与托盘资源                                     |
| `src-tauri/sidecar/`             | 生成输入  | `prepare:runtime` 下载的内核，构建时打包                 |
| `src-tauri/resources/`           | 生成输入  | Geo 数据、service 二进制等打包资源                       |
| `src-tauri/local-binaries/`      | 生成输入  | 本地 service 二进制输出                                  |
| `src-tauri/target/`              | 生成      | Rust 构建输出，不提交                                    |
| `scripts/`                       | 源码      | 构建、校验、发布脚本；入口和用途见 `scripts/README.md`   |
| `docs/`                          | 源码      | 当前说明、参考、发布流程与历史归档                       |
| `third-party/licenses/`          | 源码      | 第三方许可证原文                                         |
| `artifacts/portable/`            | 生成      | 便携包（`clash-astra_<version>_<arch>_portable*.zip`）   |
| `artifacts/staging/`             | 生成      | CI 发布/alpha 的临时 staging                             |
| `dist/`                          | 生成      | Vite 前端产物；`vite build` 会整体清空                   |
| `.toolchain/`                    | 生成      | 工作区本地 Node/pnpm 与 pnpm store/cache/state           |
| `.cargo/` `.rustup/`             | 生成      | 工作区本地 Rust 工具链与缓存                             |
| `node_modules/`                  | 生成      | 前端依赖                                                 |
| `.tmp/`                          | 生成      | 任务临时目录；wrapper 的共享临时目录为 `.tmp/host-temp/` |

## 放置规则

- 新增脚本按用途放入 `scripts/build/`、`release/`、`verify/`、`sidecars/`、`metadata/`，并同步更新 `scripts/README.md`。
- 新的发布交付物一律写入 `artifacts/portable/`；不要在 `dist/` 下创建交付物或 staging 目录。
- 新的当前说明放 `docs/architecture/`、`docs/reference/` 或 `docs/release/`；历史验证记录放 `docs/archive/`，开头注明版本与日期。
- 临时文件放 `.tmp/<任务>/`，任务结束清理；不要写入系统 Temp 或工作区外。
- 不要移动 `src-tauri/icons/`、`resources/`、`sidecar/`、`target/` 以及 `tauri.conf.json` 的 `distDir` 指向；这些路径与 Tauri 配置、CI 脚本直接绑定。
- 本机命令统一走 `node scripts/pnpm.mjs ...`；CI 使用 `pnpm/action-setup` 安装的 pnpm，两者不要把路径混用。
