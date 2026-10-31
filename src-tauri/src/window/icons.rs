//! Windows 原生图标适配：保留 EXE 中的多尺寸 ICO，分别加载窗口小图标与任务栏大图标。
//!
//! Tauri 1 的默认上下文只解码 ICO 首帧，不能承担按 DPI 选帧的职责。
//! 所有安装、替换与释放均排到窗口线程；只有本模块创建的 HICON 由本模块销毁。

use anyhow::{ensure, Context, Result};
use std::{cell::RefCell, collections::HashMap, ptr};
use tauri::Window;
use windows_sys::Win32::{
    Foundation::HWND,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
        WindowsAndMessaging::{
            DestroyIcon, IsWindow, LoadImageW, SendMessageW, HICON, ICON_BIG, ICON_SMALL,
            IMAGE_ICON, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, WM_SETICON,
        },
    },
};

/// tauri-build 将应用图标组嵌入此资源编号，使用系统资源而不是磁盘路径或缓存副本。
const APP_ICON_RESOURCE_ID: usize = 32512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Dimensions {
    width: i32,
    height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IconSizes {
    small: Dimensions,
    big: Dimensions,
}

impl IconSizes {
    fn for_dpi(dpi: u32) -> Result<Self> {
        ensure!(dpi > 0, "window DPI is unavailable");
        let sizes = unsafe {
            Self {
                small: Dimensions {
                    width: GetSystemMetricsForDpi(SM_CXSMICON, dpi),
                    height: GetSystemMetricsForDpi(SM_CYSMICON, dpi),
                },
                big: Dimensions {
                    width: GetSystemMetricsForDpi(SM_CXICON, dpi),
                    height: GetSystemMetricsForDpi(SM_CYICON, dpi),
                },
            }
        };
        ensure!(
            sizes.small.width > 0
                && sizes.small.height > 0
                && sizes.big.width > 0
                && sizes.big.height > 0,
            "Windows icon metrics are unavailable"
        );
        Ok(sizes)
    }
}

struct OwnedIcon(HICON);

impl OwnedIcon {
    fn load(size: Dimensions) -> Result<Self> {
        let module = unsafe { GetModuleHandleW(ptr::null()) };
        ensure!(module != 0, "application module handle is unavailable");
        // 不使用 LR_SHARED：每个句柄的所有权明确，替换后才能安全释放旧尺寸。
        let handle = unsafe {
            LoadImageW(
                module,
                APP_ICON_RESOURCE_ID as *const u16,
                IMAGE_ICON,
                size.width,
                size.height,
                0,
            )
        };
        ensure!(
            handle != 0,
            "load application icon {}x{} failed: {}",
            size.width,
            size.height,
            std::io::Error::last_os_error()
        );
        Ok(Self(handle))
    }
}

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        unsafe {
            DestroyIcon(self.0);
        }
    }
}

struct WindowIcons {
    sizes: IconSizes,
    small: OwnedIcon,
    big: OwnedIcon,
}

thread_local! {
    /// 按 HWND 而不是固定窗口标签持有句柄，窗口重建后旧句柄不会泄漏或复用。
    static WINDOW_ICONS: RefCell<HashMap<HWND, WindowIcons>> = RefCell::new(HashMap::new());
}

fn apply_at_dpi(hwnd: HWND, dpi: u32) -> Result<()> {
    ensure!(unsafe { IsWindow(hwnd) } != 0, "window no longer exists");
    let sizes = IconSizes::for_dpi(dpi)?;
    if WINDOW_ICONS.with(|icons| {
        icons
            .borrow()
            .get(&hwnd)
            .is_some_and(|icons| icons.sizes == sizes)
    }) {
        return Ok(());
    }
    // 两个句柄都加载成功才更新窗口，加载失败不会留下只替换一半的状态。
    let next = WindowIcons {
        sizes,
        small: OwnedIcon::load(sizes.small)?,
        big: OwnedIcon::load(sizes.big)?,
    };
    unsafe {
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, next.small.0);
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, next.big.0);
    }
    // SendMessageW 可能重入消息泵，调用期间不能持有 RefCell 借用。
    let previous = WINDOW_ICONS.with(|icons| icons.borrow_mut().insert(hwnd, next));
    drop(previous);
    log::debug!(target: "app", "window icons applied: dpi={dpi}, small={}x{}, big={}x{}", sizes.small.width, sizes.small.height, sizes.big.width, sizes.big.height);
    Ok(())
}

/// 初次建窗、恢复重建和 DPI 变化共用此入口，不依赖前端页面是否已就绪。
pub fn refresh(window: &Window) {
    let window = window.clone();
    let task_window = window.clone();
    if let Err(error) = window.run_on_main_thread(move || {
        let result = (|| -> Result<()> {
            let hwnd = task_window.hwnd().context("get native window handle")?.0 as HWND;
            let dpi = unsafe { GetDpiForWindow(hwnd) };
            apply_at_dpi(hwnd, dpi)
        })();
        if let Err(error) = result {
            log::error!(target: "app", "failed to apply native window icons: {error:#}");
        }
    }) {
        log::error!(target: "app", "failed to schedule native window icon update: {error}");
    }
}

fn release(hwnd: HWND) {
    WINDOW_ICONS.with(|icons| {
        icons.borrow_mut().remove(&hwnd);
    });
}

/// 销毁时按原始 HWND 释放，不能靠 IsWindow 判断：Destroyed 回调期间句柄可能仍有效。
pub fn install(window: &Window) {
    if let Ok(hwnd) = window.hwnd() {
        let hwnd = hwnd.0 as HWND;
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                release(hwnd);
            }
        });
    }
    refresh(window);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, mem, path::PathBuf, slice};
    use windows_sys::Win32::{
        Graphics::Gdi::{
            DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        },
        System::LibraryLoader::{FindResourceW, LoadResource, LockResource, SizeofResource},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetIconInfo, ICONINFO, WM_GETICON, WS_OVERLAPPEDWINDOW,
        },
    };

    struct HiddenWindow(HWND);
    impl HiddenWindow {
        fn new() -> Self {
            let class: Vec<_> = "STATIC\0".encode_utf16().collect();
            let title: Vec<_> = "Astra icon verification\0".encode_utf16().collect();
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPEDWINDOW,
                    0,
                    0,
                    200,
                    120,
                    0,
                    0,
                    GetModuleHandleW(ptr::null()),
                    ptr::null(),
                )
            };
            assert_ne!(hwnd, 0, "create isolated hidden window");
            Self(hwnd)
        }
    }
    impl Drop for HiddenWindow {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
            release(self.0);
        }
    }

    /// 读回系统实际持有的位图，不以源 ICO 文件的目录尺寸冒充运行时图标尺寸。
    fn pixels(handle: HICON) -> (Dimensions, Vec<u8>) {
        unsafe {
            let mut info: ICONINFO = mem::zeroed();
            assert_ne!(GetIconInfo(handle, &mut info), 0);
            let mut bitmap: BITMAP = mem::zeroed();
            assert_ne!(
                GetObjectW(
                    info.hbmColor,
                    mem::size_of::<BITMAP>() as i32,
                    &mut bitmap as *mut _ as *mut _
                ),
                0
            );
            let size = Dimensions {
                width: bitmap.bmWidth,
                height: bitmap.bmHeight.abs(),
            };
            let mut output = vec![0; (size.width * size.height * 4) as usize];
            let mut header: BITMAPINFO = mem::zeroed();
            header.bmiHeader = BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.width,
                biHeight: -size.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..mem::zeroed()
            };
            let dc = GetDC(0);
            let rows = GetDIBits(
                dc,
                info.hbmColor,
                0,
                size.height as u32,
                output.as_mut_ptr() as *mut _,
                &mut header,
                DIB_RGB_COLORS,
            );
            ReleaseDC(0, dc);
            DeleteObject(info.hbmColor);
            DeleteObject(info.hbmMask);
            assert_eq!(rows, size.height);
            (size, output)
        }
    }

    fn resource_dimensions() -> Vec<u16> {
        unsafe {
            let module = GetModuleHandleW(ptr::null());
            let resource = FindResourceW(
                module,
                APP_ICON_RESOURCE_ID as *const u16,
                14usize as *const u16,
            );
            assert_ne!(
                resource, 0,
                "compiled executable must contain the multi-size application icon group"
            );
            let loaded = LoadResource(module, resource);
            let data = slice::from_raw_parts(
                LockResource(loaded) as *const u8,
                SizeofResource(module, resource) as usize,
            );
            let count = u16::from_le_bytes([data[4], data[5]]) as usize;
            assert!(data.len() >= 6 + count * 14);
            (0..count)
                .map(|index| match data[6 + index * 14] {
                    0 => 256,
                    size => size as u16,
                })
                .collect()
        }
    }

    #[test]
    fn native_icons_use_exact_dpi_frames_and_survive_window_recreation() {
        let dimensions = resource_dimensions();
        assert_eq!(dimensions, [16, 20, 24, 32, 40, 48, 64, 72, 96, 128, 256]);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.tmp/taskbar-icon-verification")
            .join(std::process::id().to_string());
        fs::create_dir_all(&root).unwrap();
        let baseline = WINDOW_ICONS.with(|icons| icons.borrow().len());
        let mut report = Vec::new();
        for generation in 0..2 {
            let window = HiddenWindow::new();
            for (dpi, small, big) in [
                (96, 16, 32),
                (120, 20, 40),
                (144, 24, 48),
                (192, 32, 64),
                (96, 16, 32),
            ] {
                apply_at_dpi(window.0, dpi).unwrap();
                let sizes = IconSizes::for_dpi(dpi).unwrap();
                assert_eq!((sizes.small.width, sizes.big.width), (small, big));
                for (kind, label, expected) in [
                    (ICON_SMALL, "small", sizes.small),
                    (ICON_BIG, "big", sizes.big),
                ] {
                    let handle = unsafe { SendMessageW(window.0, WM_GETICON, kind as usize, 0) };
                    assert_ne!(handle, 0);
                    let (actual, bgra) = pixels(handle);
                    assert_eq!(actual, expected);
                    let source = OwnedIcon::load(expected).unwrap();
                    assert_eq!(
                        pixels(source.0).1,
                        bgra,
                        "installed icon must match the native resource at its requested size"
                    );
                    let transparent = bgra.chunks_exact(4).filter(|pixel| pixel[3] == 0).count();
                    let visible = bgra.chunks_exact(4).filter(|pixel| pixel[3] > 0).count();
                    assert!(
                        transparent > 0 && visible > 0,
                        "icon alpha must retain its transparent background and visible shape"
                    );
                    fs::write(
                        root.join(format!(
                            "generation-{generation}-{dpi}-{label}-{}x{}.bgra",
                            actual.width, actual.height
                        )),
                        &bgra,
                    )
                    .unwrap();
                    report.push(serde_json::json!({ "generation": generation, "dpi": dpi, "kind": label, "width": actual.width, "height": actual.height, "transparentPixels": transparent, "visiblePixels": visible, "resourcePixelsMatch": true }));
                }
            }
            drop(window);
            assert_eq!(
                WINDOW_ICONS.with(|icons| icons.borrow().len()),
                baseline,
                "destroyed windows must release their owned icon handles"
            );
        }
        fs::write(
            root.join("native-icons.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("native icon readback verified; reports={}", root.display());
    }
}
