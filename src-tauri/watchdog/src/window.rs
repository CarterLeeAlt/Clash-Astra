use std::time::Duration;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, TRUE},
    System::SystemInformation::GetTickCount,
    UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
    UI::WindowsAndMessaging::{
        EnumWindows, GetAncestor, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SendMessageTimeoutW,
        GA_ROOTOWNER, SMTO_ABORTIFHUNG, SMTO_BLOCK,
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

/// 应用主窗口是否是当前前台窗口（含其拥有的对话框）。
///
/// WebView2 在窗口被遮挡/切到后台时会冻结页面定时器，前端心跳会停止，
/// 这不代表进程卡死；只有前台窗口才用心跳判断 WebView 是否真的卡住。
pub fn is_foreground(hwnd: HWND) -> bool {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground == 0 {
            return false;
        }
        foreground == hwnd || GetAncestor(foreground, GA_ROOTOWNER) == hwnd
    }
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

/// 距离用户最后一次键盘/鼠标输入的时长；取不到时返回 None。
///
/// 用户长时间无输入时显示器可能已关闭或锁屏，WebView2 会把页面视为被遮挡并
/// 节流定时器，此时前端心跳变慢不代表卡死。
pub fn user_idle() -> Option<Duration> {
    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if GetLastInputInfo(&mut info) == 0 {
            return None;
        }
        Some(Duration::from_millis(u64::from(
            GetTickCount().wrapping_sub(info.dwTime),
        )))
    }
}
