# Update Log

Clash Astra 的变更日志。基于 clash-verge-rev 的上游历史变更不再在此保留，可通过 git 历史（fork 起点：Initial commit 2026-04-23）查阅。

## 1.8.2

### Added

- 测试页测试全部逐站更新并在 2 分钟后整页过期
- 运行可靠性专项修复与卡死自愈
- 测试页改为 TCP 连接探测
- 测试页站点检测增加后台定时任务
- 测试页新增 YouTube、ChatGPT、Anthropic、Grok 卡片
- 窗口标题标注管理员或普通模式
- 切换、新建与更新订阅后立即触发一次延迟测试
- 图标改为母版生成并统一替换
- release 发布说明按提交自动生成

### Changed

- 规则页 payload 列缩窄、类型列加宽
- 侧栏字标改用内置 CharterBT 22px
- 规则页 payload 列宽缩短 30%
- 发布包程序名改用用户可见名称

### Fixed

- 窗口标题去掉运行模式后缀
- 修复托盘与窗口图标在任务管理器中出现竖条裂缝
- 可靠性专项修复进程崩溃、白屏与窗口消失
- 修复看门狗误判前端卡死导致界面消失并被重新拉起
- 修复后台 WebView 冻结被误判为卡死
- 修复窗口死锁并改为手动站点检测
- 分组卡片显示选中节点延迟，自动测速完成后同步刷新
- 节点延迟按节点统一，嵌套分组隐藏选中节点名称
- 修复 exe 图标不随母版更新，侧栏改用品牌字体

## 1.8.0

### Added

- 每 2 分钟自动测速：后端调度器触发，前端批量更新节点延迟数字；代理列表直接显示最新测速结果
- 连接页默认启用进程识别（`find-process-mode: always`），表格显示发起连接的程序

### Changed

- 项目更名为 Clash Astra，应用标识、可执行文件、Windows 服务、配置目录、注册表与系统集成同步更新
- 仓库地址迁移为 `CarterLeeAlt/Clash-Astra`
- 构建工具链使用工作区本地 Node 24 / pnpm 11，pnpm store、缓存、状态与临时目录全部留在工作区
- 发布交付物统一放到 `artifacts/portable/`，`dist/` 只保留 Vite 前端产物；移除 `dist/watchdog-fix` 旧约定
- 脚本按用途整理到 `scripts/build`、`release`、`verify`、`sidecars`、`metadata`；删除无用的 `sync-version`
- 文档按架构、参考、发布与归档重组；移除手动 alpha 构建工作流，保留 `mihomo-alpha.exe` 上游内核
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
