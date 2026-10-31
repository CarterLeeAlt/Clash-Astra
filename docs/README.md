# 文档索引

本目录区分当前说明与历史记录。当前行为以代码、`package.json`、`AGENTS.md` 为准；历史记录只用于追溯，不作为操作依据。

| 文档                                                                                                                 | 内容                                         |
| -------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| [architecture/repository-layout.md](architecture/repository-layout.md)                                               | 目录职责、源码与生成物边界、文件放置规则     |
| [architecture/application-flow.md](architecture/application-flow.md)                                                 | 业务分层、配置提交与补偿、状态版本和进程归属 |
| [architecture/business-contracts.md](architecture/business-contracts.md)                                             | 基本功能契约和重构回归场景                   |
| [architecture/watchdog.md](architecture/watchdog.md)                                                                 | 独立 Watchdog 的行为、构建、测试与打包要求   |
| [reference/naming.md](reference/naming.md)                                                                           | 用户面/技术标识、固定标识、发布资产命名      |
| [reference/icons.md](reference/icons.md)                                                                             | 图标母版、生成脚本、产物与校验               |
| [release/build-and-package.md](release/build-and-package.md)                                                         | 本机构建、检查、打包与产物路径               |
| [release/windows-signing.md](release/windows-signing.md)                                                             | Windows 签名模式、CI 配置与验证              |
| [archive/watchdog-1.6.6-verification.md](archive/watchdog-1.6.6-verification.md)                                     | 1.6.6 时期的历史验证记录，命令和版本可能过期 |
| [archive/refactor-1.8.2-verification.md](archive/refactor-1.8.2-verification.md)                                     | 本次业务重构的检查结果、故障回归及未验证边界 |
| [archive/taskbar-icons-1.8.2-verification.md](archive/taskbar-icons-1.8.2-verification.md)                           | Windows 图标加载、DPI 回读及修复包验证       |
| [archive/window-size-1150x720-verification.md](archive/window-size-1150x720-verification.md)                         | 窗口默认及最小尺寸调整、回归和便携包验证     |
| [archive/current-version-cleanup-1100x700-verification.md](archive/current-version-cleanup-1100x700-verification.md) | 当前版本清理、1100×700、回归和便携包校验     |
| [archive/default-backend-tests-verification.md](archive/default-backend-tests-verification.md)                       | 后端测试默认执行、无订阅 CI 环境和实测记录   |

新增文档时：当前操作说明放 `architecture/`、`reference/` 或 `release/`；只保留验证证据的历史内容放 `archive/`，并在开头注明对应版本与日期。

订阅与固定内核的当前接口见 [Mihomo 适配契约](reference/mihomo-contract.md)。

本轮三方版本对应、真实订阅与回归结果见 [固定版本订阅适配验收](archive/mihomo-v1.19.33-subscription-verification.md)。
