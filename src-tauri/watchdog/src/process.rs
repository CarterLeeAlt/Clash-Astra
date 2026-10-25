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
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::Threading::{CreateMutexW, CREATE_NO_WINDOW},
};

use crate::{job::Job, window};

const STARTUP_TIMEOUT_SECS: u64 = 90;
const HUNG_CHECK_INTERVAL_SECS: u64 = 5;
const HUNG_CHECKS: u32 = 6;
const HEARTBEAT_CHECK_INTERVAL_SECS: u64 = 10;
const HEARTBEAT_GRACE_SECS: u64 = 60;
const HEARTBEAT_STALE_SECS: u64 = 180;
/// 窗口切到前台后给 WebView 恢复定时器的时间，避免刚获得焦点就误判。
const FOREGROUND_GRACE_SECS: u64 = 30;
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
/// - 每秒检查退出；
/// - 每 5 秒检查主窗口是否响应，连续 6 次（约 30 秒）判定卡死；
/// - 窗口可见时每 10 秒检查前端心跳，超过 120 秒判定 WebView 卡死；
/// - 启动 90 秒仍没有主窗口判定启动卡死。
pub fn supervise_child(
    exe: &Path,
    args: &[OsString],
    job: Option<&Job>,
    heartbeat: &Path,
    log: &mut dyn FnMut(&str),
) -> Outcome {
    let started = Instant::now();
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

        if window_hwnd
            .map(|hwnd| !window::is_window(hwnd))
            .unwrap_or(true)
        {
            window_hwnd = window::find_main_window(pid);
            if window_hwnd.is_some() {
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

        // 系统休眠/唤醒或时钟跳变后，心跳文件会暂时变旧；跳过一轮检查，
        // 给前端恢复心跳的时间，避免唤醒后误判为卡死。
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

        if window_hwnd.is_none()
            && now.duration_since(started) > Duration::from_secs(STARTUP_TIMEOUT_SECS)
        {
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
                    hung_checks = 0;
                }

                if hung_checks >= HUNG_CHECKS {
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
        // 仅在窗口处于前台且已持续一小段时间后判断：窗口被遮挡/切到后台时，
        // WebView2 会冻结页面定时器，心跳停止是正常现象，不能据此重启。
        let foreground_long_enough = foreground_since
            .map(|since| now.duration_since(since) >= Duration::from_secs(FOREGROUND_GRACE_SECS))
            .unwrap_or(false);
        if window_hwnd.is_some()
            && foreground_long_enough
            && now.duration_since(started) > Duration::from_secs(HEARTBEAT_GRACE_SECS)
            && now.duration_since(last_heartbeat_check)
                >= Duration::from_secs(HEARTBEAT_CHECK_INTERVAL_SECS)
        {
            last_heartbeat_check = now;
            if let Some(hwnd) = window_hwnd {
                let state = window::probe(hwnd);
                if state.visible && !state.minimized {
                    match window::heartbeat_age(heartbeat) {
                        Some(age) if age > Duration::from_secs(HEARTBEAT_STALE_SECS) => {
                            log(&format!(
                                "GUI pid={pid} frontend heartbeat stale for {}s; restarting",
                                age.as_secs()
                            ));
                            terminate_tree(&mut child, job, pid, log);
                            return Outcome::Hung {
                                visible: true,
                                uptime: started.elapsed(),
                            };
                        }
                        None => {
                            log(&format!(
                                "GUI pid={pid} frontend heartbeat file missing after startup; restarting"
                            ));
                            terminate_tree(&mut child, job, pid, log);
                            return Outcome::Hung {
                                visible: true,
                                uptime: started.elapsed(),
                            };
                        }
                        _ => {}
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(500));
    }
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
