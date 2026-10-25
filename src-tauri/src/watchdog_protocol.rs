pub const WATCHDOG_EXE: &str = "Clash Astra Watchdog.exe";
pub const CHILD_ARG: &str = "--clash-astra-watchdog-child";
pub const RESTART_EXIT_CODE: i32 = 42;
/// 看门狗在卡死恢复时追加的参数：上次卡死时窗口是可见的，重启后仍显示窗口。
pub const RECOVERED_VISIBLE_ARG: &str = "--clash-astra-recovered-visible";
/// 看门狗在卡死恢复时追加的参数：上次卡死时窗口不可见，重启后保持隐藏。
pub const RECOVERED_HIDDEN_ARG: &str = "--clash-astra-recovered-hidden";
