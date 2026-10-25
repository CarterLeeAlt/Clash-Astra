pub const WATCHDOG_EXE: &str = "Clash Astra Watchdog.exe";
pub const CHILD_ARG: &str = "--clash-astra-watchdog-child";
pub const RESTART_EXIT_CODE: i32 = 42;
/// 看门狗在卡死恢复时追加的参数：上次卡死时窗口是可见的，重启后仍显示窗口。
pub const RECOVERED_VISIBLE_ARG: &str = "--clash-astra-recovered-visible";
/// 看门狗在卡死恢复时追加的参数：上次卡死时窗口不可见，重启后保持隐藏。
pub const RECOVERED_HIDDEN_ARG: &str = "--clash-astra-recovered-hidden";
/// 看门狗使用的运行时目录（相对 exe 所在目录）；GUI 侧经 `app_home_dir()/runtime` 得到同一目录。
#[allow(dead_code)]
pub const RUNTIME_DIR: [&str; 3] = [".config", "io.github.clash-astra", "runtime"];
/// 前端心跳文件，内容是最近一次心跳的 Unix 毫秒时间戳。
///
/// 必须写入时间戳内容并由看门狗读取内容判断：反复把空文件截断为空时，
/// NTFS 不更新 LastWriteTime，按 mtime 判断会在启动约 3 分钟后误判心跳过期。
pub const HEARTBEAT_FILE: &str = "gui-heartbeat";
/// 退出标记，内容是开始退出的 Unix 毫秒时间戳；看门狗见到后停止卡死检测，
/// 避免退出清理阻塞主线程时被误判卡死并重新拉起。
pub const QUITTING_FILE: &str = "gui-quitting";
