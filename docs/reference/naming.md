# 命名规范

本项目采用“显示名 + 机器名”两层命名。展示给人看的地方用显示名，参与路径、文件名、注册表、服务和发布资产的标识用机器名。

## 两层名称

| 层     | 规范               | 用途                                                               |
| ------ | ------------------ | ------------------------------------------------------------------ |
| 显示名 | `Clash Verge Buty` | 窗口标题、托盘提示、任务管理器描述、安装界面、文档正文             |
| 机器名 | `clash-verge-buty` | 可执行文件、包名、配置名、数据目录、服务名、注册表、发布资产、日志 |

显示名保留空格与首字母大写，不写成 `Clash-Verge-Buty`、`Clash_Verge_Buty` 或全小写。机器名统一小写并用连字符分词，不写成驼峰、下划线或空格。

## 固定标识

| 项目               | 规范值                                     |
| ------------------ | ------------------------------------------ |
| 产品显示名         | `Clash Verge Buty`                         |
| npm 包名           | `clash-verge-buty`                         |
| Rust 包名          | `clash-verge-buty`                         |
| Windows 应用标识   | `io.github.clash-verge-buty`               |
| 开发版应用标识     | `io.github.clash-verge-buty.dev`           |
| 主程序             | `clash-verge-buty.exe`                     |
| Watchdog           | `clash-verge-buty-watchdog.exe`            |
| 运行配置           | `clash-verge-buty.yaml`                    |
| 检查配置           | `clash-verge-buty-check.yaml`              |
| Windows 服务名     | `clash-verge-buty-service`                 |
| Windows 服务显示名 | `Clash Verge Buty Service`                 |
| 服务 crate（内部） | `clash-verge-buty-service-src`             |
| 服务程序           | `clash-verge-buty-service.exe`             |
| 服务安装程序       | `clash-verge-buty-service-install.exe`     |
| 服务卸载程序       | `clash-verge-buty-service-uninstall.exe`   |
| 防火墙辅助程序     | `clash-verge-buty-firewall-helper.exe`     |
| 防火墙规则         | `Clash Verge Buty`、`Clash Verge Buty LAN` |
| 注册表恢复键       | `SOFTWARE\clash-verge-buty`                |
| 自启动项           | `clash-verge-buty`（由主程序文件名派生）   |
| TUN 设备           | `clash-verge-buty`                         |
| GitHub 仓库        | `CarterLeeAlt/Clash-Verge-Buty`            |

GitHub 仓库地址属于外部既有标识，保持原样，不参与本地显示名和机器名的替换。`enableLoopback.exe`、`mihomo.exe`、`mihomo-alpha.exe` 以及 GeoIP/GeoSite 数据文件属于上游第三方资源，保持原名。

## 发布资产

发布资产统一使用机器名：

```text
clash-verge-buty_<version>_<arch>_<kind>
```

- `version`：以 `package.json` 的 `version` 为准，例如 1.7.0。
- `arch`：`x64` 或 `arm64`。
- `kind`：`portable` 或 `source`。
- 未签名构建在 `kind` 后加 `_unsigned`。
- 主程序在压缩包根目录固定为 `clash-verge-buty.exe`。
- 上游内核保持原名：`mihomo.exe`、`mihomo-alpha.exe`。

示例：

```text
clash-verge-buty_<version>_x64_portable.zip
clash-verge-buty_<version>_x64_portable_unsigned.zip
clash-verge-buty_<version>_source.zip
```

本地打包输出到 `artifacts/portable/`；CI 的 release staging 在 `artifacts/staging/`。发布脚本只接受规范化后的主程序，缺失时直接失败，不再猜测其他文件名。

## 不再使用的名称

以下名称不再作为别名、回退或兼容入口：

- `Clash-Verge-Buty`
- `Clash_Verge_Buty`
- `Clash Verge Buty` 作为文件名（显示名只用于展示）
- `io.github.clash-verge-buty.data`
- `io.github.clash-verge-buty.data.dev`
- `clash-verge-service`、`clash-verge-service.exe`
- `install-service.exe`、`uninstall-service.exe`、`firewall-helper.exe`
- `Clash-Verge-Buty LAN` 防火墙规则和 `SOFTWARE\Clash-Verge-Buty` 注册表键
- `Clash-Verge-Buty` TUN 设备名
- `clash://` URL 协议注册

构建产物先由 Tauri 按显示名生成，再由 `scripts/build/normalize-main-exe.mjs` 改名为机器名。

## 检查

从仓库根执行：

```text
node scripts/pnpm.mjs run check:naming
```

该检查核对应用标识、服务名、主程序名、TUN 默认值、构建/CI 脚本和发布脚本中的旧名称。
