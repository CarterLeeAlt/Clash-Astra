use std::{
    ffi::OsString,
    io,
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::Path,
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant, SystemTime},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND},
    System::Threading::{CreateMutexW, CREATE_NO_WINDOW},
};

use crate::{
    job::Job,
    runtime_files::{unix_millis, Heartbeat, RuntimeFiles},
    window,
};

/// 启动后多久还没有出现过主窗口才判定启动卡死（开机自启时磁盘、杀毒扫描可能很慢）。
const STARTUP_TIMEOUT_SECS: u64 = 180;
const HUNG_CHECK_INTERVAL_SECS: u64 = 5;
/// 主线程连续无响应约 60 秒才判定卡死：真正的死锁会一直持续，放宽阈值几乎
/// 不影响恢复，却能避开短时阻塞（大配置加载、系统负载高）造成的误杀。
const HUNG_CHECKS: u32 = 12;
const HEARTBEAT_CHECK_INTERVAL_SECS: u64 = 10;
/// 前端心跳在这段时间内一次都没有更新才判定 WebView 卡死；判定窗口必须完全落在
/// 窗口持续处于前台的时段内，避免把后台/遮挡时被节流的定时器当成卡死。
const HEARTBEAT_STALE_SECS: u64 = 180;
/// 只有用户最近有输入时才用心跳判断：长时间无输入时可能已息屏或锁屏。
const USER_ACTIVE_SECS: u64 = 60;
const KILL_WAIT_SECS: u64 = 10;

pub struct Instance {
    mutex: HANDLE,
}

impl Instance {
    pub fn acquire(exe: &Path) -> io::Result<Option<Self>> {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in exe.to_string_lossy().to_lowercase().as_bytes() {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
        let name: Vec<u16> = format!("Local\\clash-astra-watchdog-{hash:016x}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if mutex == 0 {
            return Err(io::Error::last_os_error());
        }
        let instance = Self { mutex };
        if already_exists {
            return Ok(None);
        }
        Ok(Some(instance))
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.mutex);
        }
    }
}

pub enum Outcome {
    Exited {
        status: io::Result<ExitStatus>,
        uptime: Duration,
    },
    Hung {
        visible: bool,
        uptime: Duration,
    },
}

impl Outcome {
    pub fn uptime(&self) -> Duration {
        match self {
            Self::Exited { uptime, .. } | Self::Hung { uptime, .. } => *uptime,
        }
    }
}

/// 启动 GUI 并持续监督：
/// - 每 0.5 秒检查退出；
/// - 每 5 秒用 WM_NULL 探测主窗口，连续约 60 秒无响应判定主线程卡死；
/// - 窗口可见、持续在前台且用户在操作时，前端心跳 180 秒未更新判定 WebView 卡死；
/// - 启动 180 秒仍没有出现过主窗口判定启动卡死。
///
/// `hang_detection` 为 false（卡死重启次数已达上限）或 GUI 已开始退出时只等待退出、
/// 不再结束进程：宁可留着一个卡住的窗口，也不能让界面反复消失。
pub fn supervise_child(
    exe: &Path,
    args: &[OsString],
    job: Option<&Job>,
    files: &RuntimeFiles,
    hang_detection: bool,
    log: &mut dyn FnMut(&str),
) -> Outcome {
    let started = Instant::now();
    let started_ms = unix_millis(SystemTime::now());
    let mut child = match Command::new(exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return Outcome::Exited {
                status: Err(err),
                uptime: started.elapsed(),
            }
        }
    };
    let pid = child.id();

    if let Some(job) = job {
        let handle = child.as_raw_handle() as HANDLE;
        if let Err(err) = job.assign(handle) {
            log(&format!(
                "failed to assign GUI pid={pid} to job: {err}; falling back to taskkill for tree cleanup"
            ));
        }
    }

    let mut window_hwnd = None;
    let mut window_seen = false;
    let mut quitting = false;
    let mut last_hung_check = Instant::now();
    let mut last_heartbeat_check = Instant::now();
    let mut last_wall = SystemTime::now();
    let mut foreground_since: Option<Instant> = None;
    let mut hung_checks = 0u32;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Outcome::Exited {
                    status: Ok(status),
                    uptime: started.elapsed(),
                }
            }
            Ok(None) => {}
            Err(err) => {
                log(&format!("failed to poll GUI pid={pid}: {err}"));
                return Outcome::Exited {
                    status: Err(err),
                    uptime: started.elapsed(),
                };
            }
        }

        if !hang_detection || quitting {
            thread::sleep(Duration::from_millis(500));
            continue;
        }

        if files.quitting_since(started_ms) {
            quitting = true;
            log(&format!(
                "GUI pid={pid} is quitting; hang detection disabled until it exits"
            ));
            continue;
        }

        if window_hwnd
            .map(|hwnd| !window::is_window(hwnd))
            .unwrap_or(true)
        {
            window_hwnd = window::find_main_window(pid);
            if window_hwnd.is_some() {
                window_seen = true;
                hung_checks = 0;
                log(&format!("GUI pid={pid} main window found"));
            }
        }

        let now = Instant::now();

        if let Some(hwnd) = window_hwnd {
            if window::is_foreground(hwnd) {
                foreground_since.get_or_insert(now);
            } else {
                foreground_since = None;
            }
        } else {
            foreground_since = None;
        }

        // 系统休眠/唤醒或时钟跳变后，心跳会暂时变旧、窗口也可能短时无响应；
        // 清零所有计数，给 GUI 恢复的时间，避免唤醒后误判为卡死。
        let wall = SystemTime::now();
        let wall_gap = wall.duration_since(last_wall).unwrap_or_default();
        last_wall = wall;
        if wall_gap > Duration::from_secs(30) {
            log(&format!(
                "system resume or clock jump detected ({}s); reset hang checks",
                wall_gap.as_secs()
            ));
            hung_checks = 0;
            foreground_since = None;
            last_hung_check = now;
            last_heartbeat_check = now;
            thread::sleep(Duration::from_millis(500));
            continue;
        }

        // 只在从未出现过主窗口时判定启动卡死；窗口后来消失（异常销毁后重建中）不算。
        if !window_seen && now.duration_since(started) > Duration::from_secs(STARTUP_TIMEOUT_SECS) {
            log(&format!(
                "GUI pid={pid} produced no main window within {STARTUP_TIMEOUT_SECS}s; treating as hang"
            ));
            terminate_tree(&mut child, job, pid, log);
            return Outcome::Hung {
                visible: false,
                uptime: started.elapsed(),
            };
        }

        if now.duration_since(last_hung_check) >= Duration::from_secs(HUNG_CHECK_INTERVAL_SECS) {
            last_hung_check = now;
            if let Some(hwnd) = window_hwnd {
                let state = window::probe(hwnd);
                if state.hung {
                    hung_checks += 1;
                    log(&format!(
                        "GUI main window unresponsive check {hung_checks}/{HUNG_CHECKS}"
                    ));
                } else {
                    if hung_checks > 0 {
                        log(&format!(
                            "GUI main window responsive again after {hung_checks} failed checks"
                        ));
                    }
                    hung_checks = 0;
                }

                // 退出清理在主线程上执行，杀之前再确认一次 GUI 没有在退出。
                if hung_checks >= HUNG_CHECKS && !files.quitting_since(started_ms) {
                    log(&format!(
                        "GUI pid={pid} main window hwnd={} unresponsive for ~{}s; restarting",
                        state.hwnd,
                        HUNG_CHECK_INTERVAL_SECS * u64::from(HUNG_CHECKS)
                    ));
                    terminate_tree(&mut child, job, pid, log);
                    return Outcome::Hung {
                        visible: state.visible && !state.minimized,
                        uptime: started.elapsed(),
                    };
                }
            }
        }

        // WebView 崩溃或 JS 卡死时主线程仍可能响应，用前端心跳兜底。
        if hung_checks == 0
            && now.duration_since(last_heartbeat_check)
                >= Duration::from_secs(HEARTBEAT_CHECK_INTERVAL_SECS)
        {
            last_heartbeat_check = now;
            if let (Some(hwnd), Some(since)) = (window_hwnd, foreground_since) {
                if let Some(stale) = frontend_stale_for(files, hwnd, since, now, started_ms) {
                    log(&format!(
                        "GUI pid={pid} frontend heartbeat not updated for {}s while the window \
                         stayed in foreground and the user was active; restarting",
                        stale.as_secs()
                    ));
                    terminate_tree(&mut child, job, pid, log);
                    return Outcome::Hung {
                        visible: true,
                        uptime: started.elapsed(),
                    };
                }
            }
        }

        thread::sleep(Duration::from_millis(500));
    }
}

/// 判断前端是否卡死，返回心跳未更新的时长。
///
/// 条件全部满足才算：窗口持续在前台满判定窗口、用户最近有输入、窗口可见未最小化
/// 且主线程有响应、判定窗口内心跳一次都没有更新。心跳文件不存在按「本次启动后
/// 从未心跳」处理；文件存在但内容不完整（与 GUI 写入撞车）时跳过本轮。
fn frontend_stale_for(
    files: &RuntimeFiles,
    hwnd: HWND,
    foreground_since: Instant,
    now: Instant,
    started_ms: u64,
) -> Option<Duration> {
    let window_limit = Duration::from_secs(HEARTBEAT_STALE_SECS);
    if now.duration_since(foreground_since) < window_limit {
        return None;
    }
    if window::user_idle()? > Duration::from_secs(USER_ACTIVE_SECS) {
        return None;
    }
    let state = window::probe(hwnd);
    if state.hung || !state.visible || state.minimized {
        return None;
    }
    let last_ms = match files.heartbeat() {
        Heartbeat::At(ms) => ms.max(started_ms),
        Heartbeat::Unreadable => return None,
        Heartbeat::Missing => started_ms,
    };
    let now_ms = unix_millis(SystemTime::now());
    let stale = Duration::from_millis(now_ms.saturating_sub(last_ms));
    (stale >= window_limit).then_some(stale)
}

fn terminate_tree(child: &mut Child, job: Option<&Job>, pid: u32, log: &mut dyn FnMut(&str)) {
    if let Some(job) = job {
        if let Err(err) = job.terminate() {
            log(&format!("failed to terminate job for GUI pid={pid}: {err}"));
            kill_tree(pid, log);
        }
    } else {
        kill_tree(pid, log);
    }

    if wait_for_exit(child, Duration::from_secs(KILL_WAIT_SECS)) {
        return;
    }

    log(&format!(
        "GUI pid={pid} did not exit after job termination; taskkill /T"
    ));
    kill_tree(pid, log);
    if !wait_for_exit(child, Duration::from_secs(5)) {
        log(&format!("GUI pid={pid} still not exited after taskkill"));
    }
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(_) => return true,
        }
    }
    false
}

fn kill_tree(pid: u32, log: &mut dyn FnMut(&str)) {
    let pid = pid.to_string();
    match Command::new("taskkill")
        .args(["/PID", &pid, "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
    {
        Ok(status) => log(&format!("taskkill /T /F pid={pid} exited with {status}")),
        Err(err) => log(&format!("taskkill /T /F pid={pid} failed: {err}")),
    }
}
