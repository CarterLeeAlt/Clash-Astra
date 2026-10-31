use std::{os::windows::process::CommandExt, process::Command};

/// 外部应用由用户继续使用，不属于 GUI 的退出清理范围。
/// Watchdog 仅允许显式 BREAKAWAY，内核和 WebView 的普通子进程仍继承监督 Job。
pub fn configure_external(command: &mut Command) -> &mut Command {
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x01000000;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_BREAKAWAY_FROM_JOB | CREATE_NO_WINDOW)
}
