# Watchdog 1.6.6 验证记录（归档）

> 归档说明：以下内容记录 1.6.6 时期独立 Watchdog 的验证环境与结果。命令、版本、端口和路径可能已经过期，不作为当前操作指南。当前说明见 [Watchdog 设计与行为](../architecture/watchdog.md)。

## 验证环境

- Windows 10 x64
- Visual Studio 2022 Community 17.14.18
- Rust 1.85.1
- 工作区本地 Rust/Node 工具链；构建与缓存未写入全局路径

## 手工 GUI 场景

- 使用工作区隔离配置测试，关闭系统代理、自启和 TUN，使用 43331/17897/19097 端口。
- 启动与重复启动后恰好一个 GUI 和一个 Watchdog。
- 强制结束 GUI 后，Watchdog 自动恢复 GUI；旧核心退出，新核心重新监听 17897。

## 便携包场景

以下五种情况通过：

1. 完整包。
2. 缺少 helper。
3. helper 是目录而不是文件。
4. 只有 helper、没有 GUI。
5. GUI 文件名非标准时的 fallback。

## 图标验证

- 内嵌大/小图标提取成功。
- 实际资源为完整黄色猫（原版珊瑚橙色板）。
- 文件描述分别为 `Clash Verge Buty` 与 `Clash Verge Buty-Watchdog`。

## 行为回归

当时使用黑盒脚本验证六种场景：正常退出、失败一次后恢复、退出 42 重启、四次启动后耗尽三次重试、互斥单实例、外部应用继续存活，均通过。当前脚本路径为 `scripts/verify/test-watchdog.ps1`，但环境要求与命令以现行文档为准。
