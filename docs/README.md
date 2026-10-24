# 文档索引

本目录区分当前说明与历史记录。当前行为以代码、`package.json`、`AGENTS.md` 为准；历史记录只用于追溯，不作为操作依据。

| 文档                                                                             | 内容                                         |
| -------------------------------------------------------------------------------- | -------------------------------------------- |
| [architecture/repository-layout.md](architecture/repository-layout.md)           | 目录职责、源码与生成物边界、文件放置规则     |
| [architecture/watchdog.md](architecture/watchdog.md)                             | 独立 Watchdog 的行为、构建、测试与打包要求   |
| [reference/naming.md](reference/naming.md)                                       | 显示名/机器名、固定标识、发布资产命名        |
| [release/build-and-package.md](release/build-and-package.md)                     | 本机构建、检查、打包与产物路径               |
| [release/windows-signing.md](release/windows-signing.md)                         | Windows 签名模式、CI 配置与验证              |
| [archive/watchdog-1.6.6-verification.md](archive/watchdog-1.6.6-verification.md) | 1.6.6 时期的历史验证记录，命令和版本可能过期 |

新增文档时：当前操作说明放 `architecture/`、`reference/` 或 `release/`；只保留验证证据的历史内容放 `archive/`，并在开头注明对应版本与日期。
