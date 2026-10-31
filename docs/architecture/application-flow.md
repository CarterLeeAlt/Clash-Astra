# 业务流程与状态归属

## 前端入口

| 路径                         | 职责                                                             |
| ---------------------------- | ---------------------------------------------------------------- |
| `src/app/`                   | 应用装配、路由、主题与后端事件订阅                               |
| `src/features/`              | 按订阅、代理、规则、连接、日志、设置和站点检测组织页面与业务组件 |
| `src/infrastructure/ipc/`    | IPC 请求与统一错误解码                                           |
| `src/infrastructure/mihomo/` | 内核查询和连接控制的 HTTP 适配                                   |
| `src/shared/async/`          | 请求代次、初始化合并、可停止兜底轮询                             |
| `src/components/`            | 通用 UI、表单和布局组件                                          |
| `src/hooks/`                 | 跨页面查询和通用流 Hook                                          |
| `src/services/`              | 公共访问入口、类型声明、通知与测速展示缓存                       |

`services/api.ts` 和 `services/cmds.ts` 是公开导出入口，本身不执行业务或增加兼容分支。设置页的导航选项读取 `shared/navigation.ts`，不反向导入包含页面组件的路由树。

## 后端入口

| 路径              | 职责                                                      |
| ----------------- | --------------------------------------------------------- |
| `commands/`       | IPC 参数和返回值转换                                      |
| `application/`    | 配置、订阅、服务、窗口、托盘/热键动作和诊断任务的业务协调 |
| `domain/`         | 订阅类型约束、端口规则和窗口生命周期状态转换              |
| `config/`         | 持久化数据结构、已提交配置快照和启动读取                  |
| `infrastructure/` | 跨文件事务、订阅索引加载与保护、外部应用启动和集成验证    |
| `enhance/`        | 显式输入的配置编译、脚本与合并逻辑                        |
| `core/`           | Mihomo、Windows 平台、任务调度和托盘适配                  |
| `window/`         | 窗口创建、显示、持久化和 WebView 故障适配                 |

窗口状态转换由 `domain/window_lifecycle.rs` 维护，平台调用在状态锁释放后执行。GUI 与服务共享 `service_protocol.rs`，GUI 与 Watchdog 共享 `watchdog_protocol.rs`。

窗口内容区默认与最小尺寸均为 `1100×700` 逻辑像素，不包含系统标题栏和外边框；创建、窗口状态校验和保存门槛共用 `domain/window_geometry.rs`。全新安装使用默认尺寸并居中；日常调整后的有效窗口尺寸及最大化状态会保存并在下次启动恢复，无效记录回退到默认尺寸。不为旧版本增加尺寸兼容或迁移分支。

当前版本按全新安装交付，只支持 Mihomo 稳定版与 Alpha，不提供旧版本配置迁移或旧内核格式转换。订阅索引模板直接包含全局脚本，加载和提交均校验 UID、类型、文件名及引用，不补 UID、不改写名称、不自动插入缺失索引项。损坏或不完整的索引先留存原件再使用当前默认结构；备份失败则阻止覆盖。旧 `script` 代理模式、字符串形式的 Hysteria ALPN 明确报错，不静默转换。

首次设置模板直接写入当前七个测试站点；前端只按 UID 添加内置图标和只读标记，不按名称或地址识别旧项，不重排、补项或自动回写。用户站点的增删改和排序仍通过配置事务持久化。

## 配置提交

所有配置修改入口共用应用写入协调器，包括页面、托盘、快捷键、自动更新和窗口状态保存。

1. 取得协调器写入许可，复制当前已提交快照。
2. 对独立候选状态执行业务编辑与约束校验。
3. 需要更新内核时，读取候选订阅内容，编译配置并调用 Mihomo 校验。
4. 写入恢复记录，再更新配置文件并执行必要的外部变更。
5. 同步订阅调度任务，持久化提交标记。
6. 原子发布内存快照，再发送包含版本和受影响资源的事件。
7. 按业务需要关闭旧连接、停止空闲服务或触发测速。此类提交后的动作失败时记录诊断，不把已经提交的配置谎报为回滚成功。

候选状态不会暴露给读取方。`Config::publish` 只能由配置协调器调用。页面卸载不会取消已经接受的业务修改；退出线程先关闭入口、等待在途修改结束，再清理内核与系统代理。

## 恢复与错误

- `runtime/config-transaction.json` 保存本次变更涉及文件的原内容和目标内容。
- 提交前失败，先恢复文件，再补偿已尝试的内核、节点选择、代理、防火墙、启动项、热键与任务变更。
- 节点选择恢复使用操作前的实际内核快照，不能仅依赖订阅中可能缺失的历史选择。
- 补偿不完整时保留恢复记录；下一次启动先处理该记录，再加载配置。
- 原子替换遇到占用错误时保留原文件；损坏配置未能安全备份时阻止默认值覆盖。
- 跨文件更新与 Windows 外部效果采用补偿机制，不宣称具有数据库式整体原子性。
- IPC 错误包含 `code`、`message`、`stage`、`recoveryError`，前端转换为 `CommandError`。
- 窗口控件的乐观值保留在控件内；成功后才更新已提交数据缓存。

## 版本与异步任务

- `revision`：每次配置提交递增。
- `runtime_revision`：运行时操作成功提交时递增；相同内容的订阅重载也需要使旧请求失效。
- `profile_versions`：逐订阅计版本；下载结束后校验，拒绝覆盖期间发生的编辑或更新。
- 内核 `generation`：用于区分进程实例和过期终止事件。
- 站点检测使用 `runId`、`sequence`、运行时版本和内核代次约束结果顺序。
- WS 恢复会停止兜底轮询，并使在途旧请求失效；停止后的请求不能自行重新调度。
- 自动节点检测会合并运行期间的新请求；过期运行时的结果不发布。
- 整轮站点检测由后端承担 60 秒预算；超时取消未完成子任务并保留已有结果。

## 进程归属

Watchdog 先以挂起方式创建 GUI，将它加入 Job 后再恢复执行，消除子进程抢先逃离监督的启动竞态。Mihomo 与 WebView 普通子进程继承 Job；打开浏览器、目录和编辑器使用显式脱离 Job 的桌面适配器。Job 允许显式脱离，但不允许所有子进程自动脱离。

## 检查入口

通过工作区 Node 与 pnpm 入口运行：

```powershell
node scripts/pnpm.mjs exec tsc --noEmit
node scripts/pnpm.mjs run test:frontend
node scripts/pnpm.mjs run check:architecture
node scripts/pnpm.mjs exec cargo test --manifest-path src-tauri/Cargo.toml --bin clash-astra
node scripts/pnpm.mjs exec cargo check --manifest-path src-tauri/windows-service-src/Cargo.toml
node scripts/check-workspace-hygiene.mjs
```

真实内核集成测试默认忽略；通过进程级 `ASTRA_TEST_SUBSCRIPTION_FILE` 指定工作区内订阅文件后，可单独执行 `subscription_compile_reload_and_http_rejection` 测试。它使用独立目录和临时监听端口，不更改 Windows 系统代理或服务。测试订阅与结果配置不进入版本控制。

变更业务状态时应同步维护 [业务行为与验收](business-contracts.md)，并添加能验证失败、并发或生命周期边界的回归场景。
