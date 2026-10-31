# 图标规范

图标由 `assets/brand/` 的母版经一个脚本生成。仓库里的图标文件都是生成物，不手工修改。

## 母版

| 文件                    | 规格                                | 用途                        |
| ----------------------- | ----------------------------------- | --------------------------- |
| `app-icon.png`          | 1024×1024，PNG-24 + Alpha，sRGB     | 应用图标、favicon、侧栏图标 |
| `watchdog-icon.png`     | 同上                                | Watchdog 进程图标           |
| `tray-normal.png`       | 30×30 PNG，透明（换新图建议 32×32） | 托盘普通状态                |
| `tray-system-proxy.png` | 同上                                | 系统代理开启                |
| `tray-tun.png`          | 同上                                | TUN 模式                    |

母版统一 1024×1024（覆盖 256px 输出仍有 4 倍余量），更大的正方形母版同样接受，小于 1024 会被脚本拒绝。母版要求：正方形、图形居中、四周留 6%～ 8% 安全边距、不带文字；托盘图标要在 16px 下仍能分辨，颜色不宜多。托盘产物与母版同尺寸、不做缩放：当前沿用旧版 30×30 图标，换新图时按 32×32 提供，不要裁切图形并留出透明边距。

当前应用与 Watchdog 两张母版采用用户明确确认的小边距构图，按原图缩放生成，透明边距约 7 ～ 14 px（以 Alpha ≥128 的可见范围统计）。这两张图作为上述 6%～ 8% 边距要求的特定例外；生成时保留图案大小与位置，不自动加留白。16 px 下 Watchdog 盾牌的细节较弱，尺寸验收以图案完整、透明背景和原生帧正确加载为准。

## 生成与校验

```powershell
# 重新生成全部图标
node scripts/pnpm.mjs run icons

# 校验生成物与母版一致（CI 构建前执行）
node scripts/pnpm.mjs run icons:check
```

脚本 `scripts/build/build-icons.ps1` 读取母版并写入 `src-tauri/icons/` 与 `src/assets/image/`。`icons:check` 逐像素比对生成物与母版渲染结果，并检查 `src-tauri/icons/` 里没有清单外的文件。

## 产物

| 文件                                                         | 规格                 | 使用方                                             |
| ------------------------------------------------------------ | -------------------- | -------------------------------------------------- |
| `src-tauri/icons/icon.ico`                                   | 16 ～ 256，11 帧     | EXE 原生资源及按 DPI 加载的标题栏、任务栏、Alt+Tab |
| `src-tauri/icons/32x32.png`、`128x128.png`、`128x128@2x.png` | 32/128/256           | `tauri.conf.json` 的 `bundle.icon`                 |
| `src-tauri/icons/watchdog.ico`                               | 16 ～ 256，11 帧     | `watchdog/build.rs` 嵌入 Watchdog                  |
| `src-tauri/icons/tray-icon.png`                              | 30×30                | 托盘普通状态，`core/tray.rs` 编译期嵌入            |
| `src-tauri/icons/tray-icon-sys.png`                          | 30×30                | 系统代理开启                                       |
| `src-tauri/icons/tray-icon-tun.png`                          | 30×30                | TUN 模式                                           |
| `src/assets/image/favicon.ico`                               | 16/20/24/32/40/48/64 | `src/index.html`                                   |
| `src/assets/image/brand-icon.png`                            | 56×56                | 侧栏品牌区，按 28px 显示                           |

侧栏的产品名取自 `package.json` 的 `metadata.productName`，不再维护字标 SVG。

## Windows 运行时加载

应用和 Watchdog 的 ICO 帧尺寸统一为 `16、20、24、32、40、48、64、72、96、128、256`，20/40 覆盖 125% DPI。ICO 保持从小到大的自然顺序，不通过调整首帧规避加载问题。

Tauri 1 在 Windows 上默认只解码 ICO 第一帧。主程序关闭这条默认窗口图标入口，由 `src-tauri/src/window/icons.rs` 从 EXE 中编号 `32512` 的完整图标组读取，分别设置 `ICON_SMALL` 和 `ICON_BIG`。首次建窗、WebView 恢复重建及 `ScaleFactorChanged` 使用相同入口；图标句柄在窗口线程创建、替换和释放。

| 缩放 | DPI | 窗口小图标 | 任务栏大图标 |
| ---- | --- | ---------- | ------------ |
| 100% | 96  | 16×16      | 32×32        |
| 125% | 120 | 20×20      | 40×40        |
| 150% | 144 | 24×24      | 48×48        |
| 200% | 192 | 32×32      | 64×64        |

实际尺寸读取 Windows 对应 DPI 的系统指标。`icons:check` 验证母版与全部帧；Rust 的 `native_icons_use_exact_dpi_frames_and_survive_window_recreation` 用独立隐藏窗口通过 `WM_GETICON`、`GetIconInfo` 和 `GetDIBits` 验证实际尺寸、像素、透明度以及窗口销毁后的资源释放，不启动代理业务。

托盘是独立路径：保留现有 30×30 母版与三种状态含义，不通过放大托盘母版伪造更高分辨率。浏览器 favicon 和侧栏品牌图片不控制原生任务栏图标。

## 规则

- 只改 `assets/brand/` 母版，然后重跑 `icons`；不手工替换任何生成物。
- `src-tauri/icons/` 只允许出现上表中的文件，多出文件 `icons:check` 直接失败。
- 每个图标只有一个来源，不保留同名副本、备份或中间产物。
- 托盘图标支持运行时覆盖：`.config/io.github.clash-astra/icons/` 下的 `common.png`、`sysproxy.png`、`tun.png` 在设置里对应开关打开时优先生效。
- 换图标只覆盖母版并重跑 `icons`，不需要改脚本或代码。
