# 图标规范

图标由 `assets/brand/` 的母版经一个脚本生成。仓库里的图标文件都是生成物，不手工修改。

## 母版

| 文件                    | 规格                            | 用途                        |
| ----------------------- | ------------------------------- | --------------------------- |
| `app-icon.png`          | 1024×1024，PNG-24 + Alpha，sRGB | 应用图标、favicon、侧栏图标 |
| `watchdog-icon.png`     | 同上                            | Watchdog 进程图标           |
| `tray-normal.png`       | 32×32 PNG，透明                 | 托盘普通状态                |
| `tray-system-proxy.png` | 32×32 PNG，透明                 | 系统代理开启                |
| `tray-tun.png`          | 32×32 PNG，透明                 | TUN 模式                    |

母版统一 1024×1024（覆盖 256px 输出仍有 4 倍余量），更大的正方形母版同样接受，小于 1024 会被脚本拒绝。母版要求：正方形、图形居中、四周留 6%～ 8% 安全边距、不带文字；托盘图标要在 16px 下仍能分辨，颜色不宜多。

## 生成与校验

```powershell
# 重新生成全部图标
node scripts/pnpm.mjs run icons

# 校验生成物与母版一致（CI 构建前执行）
node scripts/pnpm.mjs run icons:check
```

脚本 `scripts/build/build-icons.ps1` 读取母版并写入 `src-tauri/icons/` 与 `src/assets/image/`。`icons:check` 逐像素比对生成物与母版渲染结果，并检查 `src-tauri/icons/` 里没有清单外的文件。

## 产物

| 文件                                                         | 规格            | 使用方                                                  |
| ------------------------------------------------------------ | --------------- | ------------------------------------------------------- |
| `src-tauri/icons/icon.ico`                                   | 16 ～ 256，9 帧 | Tauri 构建嵌入 exe：标题栏、任务栏、资源管理器、Alt+Tab |
| `src-tauri/icons/32x32.png`、`128x128.png`、`128x128@2x.png` | 32/128/256      | `tauri.conf.json` 的 `bundle.icon`                      |
| `src-tauri/icons/watchdog.ico`                               | 16 ～ 256，6 帧 | `watchdog/build.rs` 嵌入 Watchdog                       |
| `src-tauri/icons/tray-icon.png`                              | 32×32           | 托盘普通状态，`core/tray.rs` 编译期嵌入                 |
| `src-tauri/icons/tray-icon-sys.png`                          | 32×32           | 系统代理开启                                            |
| `src-tauri/icons/tray-icon-tun.png`                          | 32×32           | TUN 模式                                                |
| `src/assets/image/favicon.ico`                               | 16/32/48        | `src/index.html`                                        |
| `src/assets/image/brand-icon.png`                            | 56×56           | 侧栏品牌区，按 28px 显示                                |

侧栏的产品名取自 `package.json` 的 `metadata.productName`，不再维护字标 SVG。

## 规则

- 只改 `assets/brand/` 母版，然后重跑 `icons`；不手工替换任何生成物。
- `src-tauri/icons/` 只允许出现上表中的文件，多出文件 `icons:check` 直接失败。
- 每个图标只有一个来源，不保留同名副本、备份或中间产物。
- 托盘图标支持运行时覆盖：`.config/io.github.clash-astra/icons/` 下的 `common.png`、`sysproxy.png`、`tun.png` 在设置里对应开关打开时优先生效。
- 换图标只覆盖母版并重跑 `icons`，不需要改脚本或代码。
