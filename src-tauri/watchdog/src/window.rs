use std::{
    ffi::OsStr,
    path::Path,
    time::{Duration, SystemTime},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, TRUE},
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
        IsWindow, IsWindowVisible, SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_BLOCK,
    },
};

#[derive(Clone, Copy, Debug)]
pub struct WindowState {
    pub hwnd: HWND,
    pub visible: bool,
    pub minimized: bool,
    pub hung: bool,
}

struct Search {
    pid: u32,
    found: HWND,
}

unsafe extern "system" fn enum_window(
    hwnd: HWND,
    lparam: LPARAM,
) -> windows_sys::Win32::Foundation::BOOL {
    let search = &mut *(lparam as *mut Search);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid != search.pid {
        return TRUE;
    }

    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return TRUE;
    }

    let mut buf = vec![0u16; len as usize + 1];
    let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    if copied <= 0 {
        return TRUE;
    }

    let title = String::from_utf16_lossy(&buf[..copied as usize]);
    if title.starts_with("Clash Astra") {
        search.found = hwnd;
        return 0;
    }

    TRUE
}

/// 找到 GUI 的主窗口（按 PID + 标题前缀）。隐藏窗口也能找到。
pub fn find_main_window(pid: u32) -> Option<HWND> {
    let mut search = Search { pid, found: 0 };
    unsafe {
        EnumWindows(Some(enum_window), &mut search as *mut Search as LPARAM);
    }
    if search.found == 0 {
        None
    } else {
        Some(search.found)
    }
}

pub fn is_window(hwnd: HWND) -> bool {
    unsafe { IsWindow(hwnd) != 0 }
}

pub fn probe(hwnd: HWND) -> WindowState {
    unsafe {
        // 用带超时的 WM_NULL 探测，而不是 IsHungAppWindow：后者在目标线程被
        // 挂起时可能长时间不返回，探测本身就会卡住看门狗。
        let mut result = 0usize;
        let ok = SendMessageTimeoutW(
            hwnd,
            0,
            0,
            0,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            1000,
            &mut result,
        );
        WindowState {
            hwnd,
            visible: IsWindowVisible(hwnd) != 0,
            minimized: IsIconic(hwnd) != 0,
            hung: ok == 0,
        }
    }
}

/// 前端心跳文件的年龄；文件不存在或读取失败时返回 None。
pub fn heartbeat_age(path: &Path) -> Option<Duration> {
    let modified = std::fs::metadata(OsStr::new(path)).ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}
