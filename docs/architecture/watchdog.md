# Watchdog 设计与行为

独立 Watchdog 负责监督 GUI 进程并在异常退出后恢复；它与 GUI 一起构成便携包，不能单独使用。

## 组成与命名

| 组件     | 文件                             | 说明                               |
| -------- | -------------------------------- | ---------------------------------- |
| GUI      | `clash-astra.exe`                | 只承载界面与业务，不再内置监督逻辑 |
| Watchdog | `clash-astra-watchdog.exe`       | 独立监督进程，文件名全小写         |
| 内核     | `mihomo.exe`、`mihomo-alpha.exe` | 上游内核，保持原名                 |

显示名、机器名和注册表/服务名的唯一规范见 [命名规范](../reference/naming.md)。

## 行为

- 正常退出：GUI 清理系统代理与核心后退出 0，Watchdog 随之结束。
- 托盘重启：GUI 清理后退出 42，现有 Watchdog 立即重新启动 GUI，不产生第二个监督程序。
- 异常退出：最多重试三次，每次等待十秒；超过次数后 Watchdog 退出。
- 单实例：同名互斥体保证同一时刻只有一个 Watchdog；重复启动的监督程序直接退出，不启动 GUI。
- 外部应用：通过托盘或外部方式启动的其他程序不会被 Watchdog 结束。
- “系统代理守卫”仍由 GUI 负责，与 Watchdog 的职责分离。
- 启动时依据前次核心 PID、可执行文件路径和 `-d` 配置目录核验并清理残留 Mihomo。
- 日志位于应用配置目录下的 `logs/watchdog/`（`.config/io.github.clash-astra/logs/watchdog/`）。

## 图标与显示名

- GUI 使用完整蓝猫图标；Watchdog 使用同一形状的黄色版本。黄色取自仓库自带 `tray-icon-sys.png` 的原始色板，`tray-icon-sys.png` 本身不修改。
- 图标由 `scripts/build/generate-watchdog-icon.ps1` 生成，输出 16、32、48、64、128、256 像素 ICO 与 PNG。
- GUI 的任务管理器显示名来自 `src-tauri/Cargo.toml` 的 `description`；Watchdog 的显示名在 `src-tauri/watchdog/build.rs` 中固定为 `Clash Astra-Watchdog`。

## 构建

本机使用工作区工具链，命令从仓库根执行：

```powershell
# 完整构建：前端、Tauri release、Watchdog、主程序改名
node scripts/pnpm.mjs run build

# 只构建 Watchdog
node scripts/pnpm.mjs run build:watchdog
```

开发模式下 `tauri.conf.json` 的 `beforeDevCommand` 会自动构建并放置 debug helper。构建、签名和打包的完整流程见 [构建与打包](../release/build-and-package.md)。

## 测试

Watchdog 没有单元测试，行为验收使用黑盒回归：

```powershell
pwsh -File scripts/verify/test-watchdog.ps1
```

脚本覆盖正常退出、失败一次后恢复、退出 42 重启、四次启动后耗尽三次重试、互斥单实例、外部应用继续存活六种场景。直接运行前需要把工作区 `.cargo\bin` 前置到 `PATH` 并设置 `CARGO_HOME`、`RUSTUP_HOME`、`TEMP`；环境设置见 `AGENTS.md`。测试产物写入 `.tmp/` 下的独立目录。

## 打包

- GUI 与 Watchdog 两个 EXE 必须一起分发；升级时保留现有 `.config` 目录，不覆盖订阅和个人配置。
- 便携包由 `scripts/release/portable.mjs` 生成，输出到 `artifacts/portable/`，包含 GUI、Watchdog、两个内核和 `resources/`。
- 未签名构建的文件名带 `_unsigned` 后缀；签名模式见 [Windows 签名](../release/windows-signing.md)。

  1.6.6 时期的历史验证记录保存在 [归档](../archive/watchdog-1.6.6-verification.md)，其中的命令、版本和测试端口不代表当前环境。
