# tao 0.16.7（修补版）

来源：crates.io 发布的 `tao` 0.16.7 原样复制，只保留构建需要的 `Cargo.toml`、`build.rs`、`src/` 与许可证文件；经 `src-tauri/Cargo.toml` 的 `[patch.crates-io]` 替换依赖树中的 tao（tauri 1.5 → tauri-runtime-wry 0.14 → wry 0.24 → tao 0.16）。

## 修补内容

`Cargo.toml` 末尾加了 `[lints.rust] warnings = "allow"`：路径依赖的警告不会像 registry 依赖那样被 cargo 屏蔽，上游代码的弃用警告会混进本项目的构建输出。

源码只改了 `src/platform_impl/windows/icon.rs` 中 RGBA 转 Windows 图标的 AND 掩码（新增 `and_mask_from_rgba`）：

- 上游把掩码按每像素 1 字节传给 `CreateIcon`，而 `CreateIcon` 要求每像素 1 位、MSB 在前、每行按 WORD 对齐。Windows 按 1 位/像素解读后，掩码变成第 7、15、23… 列的竖条透明，任务管理器等按掩码绘制图标的地方会显示裂缝（托盘图标最明显）。
- 修补后按规范生成 1 位掩码，alpha < 128 的像素置为透明。按 alpha 通道绘制的地方（托盘区、任务栏）不受影响。

截至修补时，上游 tao 开发分支仍是原写法。

## 验证方法

用一个独立小程序调用 `tao::window::Icon::from_rgba` 设置窗口图标，再经 `WM_GETICON` + `GetIconInfo` + `GetDIBits` 读回系统中的实际掩码，与 alpha 逐像素比对。覆盖 `src-tauri/icons/` 下的托盘与应用图标，以及宽度为 17、30、33 的合成图标（检验行对齐）：修补前全部不符，修补后全部一致。

## 升级

tauri 升级带来新的 tao 版本时，先确认上游是否已修复：已修复则删除本目录和 `[patch.crates-io]` 条目；未修复则按新版本重新复制并移植上述改动。
