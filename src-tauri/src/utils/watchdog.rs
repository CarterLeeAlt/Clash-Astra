use crate::watchdog_protocol::{CHILD_ARG, WATCHDOG_EXE};
use std::{ffi::OsStr, io, process::Command};

pub fn is_supervised() -> bool {
    if !std::env::args_os().any(|arg| arg == OsStr::new(CHILD_ARG)) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Ok(expected) = dunce::canonicalize(exe.with_file_name(WATCHDOG_EXE)) else {
        return false;
    };
    let mut system = sysinfo::System::new();
    let pid = sysinfo::Pid::from_u32(std::process::id());
    system.refresh_process(pid);
    let Some(parent) = system.process(pid).and_then(|process| process.parent()) else {
        return false;
    };
    system.refresh_process(parent);
    system
        .process(parent)
        .and_then(|process| process.exe())
        .and_then(|path| dunce::canonicalize(path).ok())
        .map(|path| {
            path.to_string_lossy()
                .eq_ignore_ascii_case(&expected.to_string_lossy())
        })
        .unwrap_or(false)
}

pub fn launch_supervisor() -> io::Result<bool> {
    // 开发模式下不启用监督进程：tauri dev 会在被监管的 GUI 退出后结束会话并停掉
    // Vite，而 watchdog 拉起的第二个实例的 webview 仍指向已关闭的 dev 服务器，
    // 结果是白屏窗口与脱离 tauri dev 的残留进程。
    if cfg!(debug_assertions) {
        return Ok(false);
    }

    if is_supervised() {
        return Ok(false);
    }

    let exe = std::env::current_exe()?;
    let helper = exe.with_file_name(WATCHDOG_EXE);
    if !helper.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("missing supervisor: {}", helper.display()),
        ));
    }

    Command::new(helper)
        .arg(exe)
        .args(std::env::args_os().skip(1))
        .spawn()?;
    Ok(true)
}
