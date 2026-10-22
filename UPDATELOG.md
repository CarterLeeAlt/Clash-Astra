# Update Log

Clash-Verge-Buty 的变更日志。基于 clash-verge-rev 的上游历史变更不再在此保留，可通过 git 历史（fork 起点：Initial commit 2026-04-23）查阅。

## Unreleased

### Added

- 连接页默认启用进程识别（`find-process-mode: always`），表格显示发起连接的程序

### Changed

- 界面英文字体使用内置 Bitstream Charter（`CharterBT`，Regular / Bold），中日韩与等宽字符沿用 Noto 系列
- 仅保留 Windows 平台：移除 macOS/Linux 死代码、平台专属资源与过期文档，修正 issue 模板与元数据署名

### Fixed

- 日志页历史段顺序：拉取历史后与批量合并的反转相互抵消，导致每次打开页面或重连后
  历史日志由旧到新排列；现与实时日志统一为最新在最前
- 连接页与日志页的刷新稳定性：快照合并改为 O(n)，新增 WS 心跳看门狗与断线兜底轮询，
  日志写入按 200ms 批量合并
- 图标渲染：`btoa` 对非 Latin1 字符抛异常，含中文的 SVG 图标会导致整页白屏
- 切换界面语言时不再被强制跳回起始页
- 开发模式下 WS 连接：`unmountedRef` 在 StrictMode 双重挂载后未复位，导致连接无法建立

## 1.6.8

- 便携包主程序统一使用小写文件名 `clash-verge-buty.exe`

## 1.6.6

- 拆出独立的 `clash-verge-buty-watchdog.exe` 监督进程，负责异常退出重启与托盘重启

## 1.6.0

- CI 在 push 时自动递增版本并发布

## 1.5.6

- 基于 clash-verge-rev v1.5.6 的 Windows 化 fork 版本
