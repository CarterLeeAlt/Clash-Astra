# 命名规范

本项目采用「用户面」与「技术标识」两层命名。判据是用户会不会直接接触这个名字，而不是它出现在界面还是路径里。

## 两层名称

| 层       | 规范          | 用途                                                                                     |
| -------- | ------------- | ---------------------------------------------------------------------------------------- |
| 用户面   | `Clash Astra` | 窗口标题、托盘提示、任务管理器描述、快捷方式，以及发布包根目录里用户直接运行的程序文件名 |
| 技术标识 | `clash-astra` | 包名、应用标识、服务短名、配置名、数据目录、注册表、TUN 设备、日志、仓库与发布资产       |

用户面名称逐字写作 `Clash Astra`，不写成 `Clash-Astra`、`Clash_Astra`、`ClashAstra` 或全小写。对外技术标识统一小写并用连字符分词；代码标识符遵循 Rust、TypeScript 各自的命名约定。

便携包解压后，文件基名就是用户看到的应用名：右键发送到桌面快捷方式时，快捷方式名取自文件基名。因此发布包根目录里的程序必须使用用户面名称，不能沿用技术标识。

## 固定标识

| 项目               | 规范值                              |
| ------------------ | ----------------------------------- |
| 产品显示名         | `Clash Astra`                       |
| 主程序             | `Clash Astra.exe`                   |
| Watchdog           | `Clash Astra Watchdog.exe`          |
| npm 包名           | `clash-astra`                       |
| Rust 包名          | `clash-astra`                       |
| Windows 应用标识   | `io.github.clash-astra`             |
| 开发版应用标识     | `io.github.clash-astra.dev`         |
| 应用设置文件       | `clash-settings.yaml`               |
| 运行配置           | `clash-astra.yaml`                  |
| 检查配置           | `clash-astra-check.yaml`            |
| Windows 服务名     | `clash-astra-service`               |
| Windows 服务显示名 | `Clash Astra Service`               |
| 服务 crate（内部） | `clash-astra-service-src`           |
| 服务程序           | `clash-astra-service.exe`           |
| 服务安装程序       | `clash-astra-service-install.exe`   |
| 服务卸载程序       | `clash-astra-service-uninstall.exe` |
| 防火墙辅助程序     | `clash-astra-firewall-helper.exe`   |
| 防火墙规则         | `Clash Astra`、`Clash Astra LAN`    |
| 注册表恢复键       | `SOFTWARE\clash-astra`              |
| 自启动项           | `Clash Astra`（由主程序文件名派生） |
| TUN 设备           | `clash-astra`                       |
| GitHub 仓库        | `CarterLeeAlt/Clash-Astra`          |

`resources/` 下的服务程序只由主程序调用，用户不会直接运行，因此保持技术标识；主程序与 Watchdog 位于发布包根目录，使用用户面名称。`enableLoopback.exe`、锁定的 `mihomo.exe` 以及 GeoIP/GeoSite 数据文件属于上游第三方资源，保持原名。

## 文件名与代码

- 用户面文件名逐字使用 `Clash Astra`，空格保留，不加连字符、下划线或驼峰变体。
- 代码中的路径一律当参数传递（`PathBuf`、`Command::new`、PowerShell 数组）；需要拼成字符串时必须整体加引号，不为空格写兼容分支。
- 不保留旧文件名的别名、回退或迁移入口；旧名出现在构建脚本、发布脚本或发布包里即视为错误。
- 主程序名直接取 Tauri 的 `productName` 产物，构建过程不做改名。Cargo 的二进制目标名不能含空格，Watchdog 由 `scripts/build/build-watchdog.mjs` 复制成发布名。

## 发布资产

发布资产统一使用技术标识：

```text
clash-astra_<version>_<arch>_<kind>
```

- `version`：以 `package.json` 的 `version` 为准，例如 1.8.0。
- `arch`：`x64` 或 `arm64`。
- `kind`：`portable` 或 `source`。
- 未签名构建在 `kind` 后加 `_unsigned`。
- 压缩包根目录的主程序为 `Clash Astra.exe`，Watchdog 为 `Clash Astra Watchdog.exe`。
- 上游内核保持原名：`mihomo.exe`，交付身份由 `mihomo.lock.json` 约束。

示例：

```text
clash-astra_<version>_x64_portable.zip
clash-astra_<version>_x64_portable_unsigned.zip
clash-astra_<version>_source.zip
```

本地打包输出到 `artifacts/portable/`；CI 的 release staging 在 `artifacts/staging/`。发布脚本只接受上面列出的主程序文件名，缺失时直接失败，不再猜测其他文件名。

## 内部职责命名

应用设置与 Mihomo 内核配置使用不同名称，避免相互混淆。

| 职责              | 代码与文件                                                                  |
| ----------------- | --------------------------------------------------------------------------- |
| 应用设置数据      | `IClashSettings`、`clashSettings`、`clash_settings`                         |
| 应用设置前端入口  | `useClashSettings`、`use-clash-settings.ts`                                 |
| 应用设置界面      | `SettingApplication`、`setting-application.tsx`，显示“应用设置”与“应用版本” |
| 应用设置 IPC      | `get_clash_settings`、`patch_clash_settings`、`clash-settings.ts`           |
| Mihomo 内核配置   | `clashConfig`、`IClashTemp`，保持与应用设置分离                             |
| 开发构建 feature  | `clash-dev`                                                                 |
| Tauri 内部事件    | `clash://...`，不是 Windows URL 协议注册                                    |
| 下载环境变量      | `CLASH_DOWNLOAD_*`                                                          |
| WebView2 环境变量 | `CLASH_WEBVIEW2_*`                                                          |

自有运行代码、配置键、文件名及当前界面只使用当前名称。第三方版权声明、真实上游地址及归档历史记录保留事实原文。

## 检查

从仓库根执行：

```text
node scripts/pnpm.mjs run check:naming
```

该检查核对应用标识、服务名、主程序与 Watchdog 文件名、TUN 默认值、应用设置契约，以及自有源码、文件名、构建脚本和当前仓库链接中的命名一致性。
