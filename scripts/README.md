# scripts

构建、校验与发布脚本。除特别说明外，所有命令都从仓库根目录执行；pnpm 相关命令统一通过 `node scripts/pnpm.mjs <命令>` 进入。

## 入口

| 入口                            | 用途                                                                                           |
| ------------------------------- | ---------------------------------------------------------------------------------------------- |
| `pnpm.mjs`                      | 工作区工具链入口：使用本地 Node/pnpm，固定 pnpm store/cache/state、`TEMP` 与 `XDG_*` 到工作区  |
| `check-workspace-hygiene.mjs`   | 校验工具链、lockfile、store/cache/state 与 `.gitignore` 约定                                   |
| `release/release-notes.mjs`     | 按 tag 区间生成发布说明，可同步写入 `UPDATELOG.md`                                             |
| `build/build-icons.ps1`         | 由 `assets/brand/` 母版生成全部图标；`-Check` 校验生成物是否漂移                               |
| `verify/check-architecture.mjs` | 校验配置单写入口、纯配置编译、领域层依赖和结构化 IPC 边界                                      |
| `verify/test-backend.mjs`       | `test:backend` 的完整后端测试入口：启动无订阅 Mihomo、核对就绪、默认运行全部测试并清理自建进程 |

## 目录

| 目录        | 内容                                                                                     |
| ----------- | ---------------------------------------------------------------------------------------- |
| `build/`    | 构建辅助：`build-watchdog.mjs`、`build-icons.ps1`                                        |
| `release/`  | 发布打包：`portable.mjs`；发布说明：`release-notes.mjs`                                  |
| `verify/`   | 校验：`check-naming.mjs`、`test-watchdog.ps1`、`test-backend.mjs`                        |
| `sidecars/` | 运行时资源准备：`prepare-runtime.mjs`，下载 mihomo/Geo 数据并构建 Windows service 二进制 |
| `metadata/` | 元数据同步：`sync-metadata.mjs`                                                          |
| `lib/`      | 共享模块：`workspace-env.mjs`                                                            |

## 约定

- package.json 的 scripts 是唯一命令入口；不要在其他文档里另写一套参数。
- 构建产物：`dist/`（前端）、`src-tauri/target/`（Rust）；发布交付物：`artifacts/portable/`；CI staging：`artifacts/staging/`。
- 临时目录一律放 `.tmp/`，任务结束后清理。

## 固定内核

`sidecars/prepare-mihomo.mjs` 读取 `src-tauri/mihomo.lock.json`，只准备 `v1.19.33` 的固定 Windows x64 资产并核对压缩包、二进制和版本。`verify/check-mihomo.mjs` 在 Tauri 生产构建前检查已有文件；打包和运行前同样核对身份。不下载最新版、不交付 Alpha、不提供软件内核升级。服务构建资源按内容与本次源码构建结果同步，不能因旧文件存在而跳过更新。

`src-tauri/mihomo.lock.json` 是内核身份的唯一锁定来源，构建、打包和运行前均使用它校验 Mihomo。便携包不携带锁定文件或源码指纹文件。完整边界见 `docs/reference/mihomo-contract.md`。

## 完整后端测试

`node scripts/pnpm.mjs run test:backend` 启动独立无订阅 Mihomo，默认监听 `127.0.0.1:7897`、规则为 `MATCH,DIRECT`，再执行同一套完整后端测试。端口占用时直接失败，不停止或接管已有实例；本机已有代理运行时可显式用 `--mixed-port 0` 选择临时空闲端口。

网络受限的本机可显式传 `--upstream-proxy http://127.0.0.1:20081`；只接受无认证信息的 HTTP 代理端点，不读取环境变量自动选择上游，也不在失败后切换路由。CI 不传该参数，使用 runner 自身的 DIRECT 出口。测试失败、忽略项不为零或进程清理失败均返回非零；每轮日志和结果保存在 `.tmp/backend-tests/<运行 UUID>/`。正常退出、失败、超时或可处理的中断均清理该入口创建的命令树，不按进程名终止用户程序。
