#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod process;
#[path = "../../src/watchdog_protocol.rs"]
mod protocol;

use protocol::{CHILD_ARG, RESTART_EXIT_CODE, WATCHDOG_EXE};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_RETRIES: usize = 3;
const RETRY_DELAY_SECS: u64 = 10;
/// 应用请求重启后的等待时间，避免立即重新拉起造成紧密循环。
const RESTART_DELAY_SECS: u64 = 2;
/// 单位时间窗内允许的「应用请求重启」次数上限。
///
/// 退出码 42 表示应用主动请求重启，正常只来自托盘的「重启应用」。
/// 若某条路径变成「启动即请求重启」，不做限频就会形成零延迟无限重启并
/// 绕开 MAX_RETRIES；这里用滑动窗口把这种情况也纳入重试计数。
const RESTART_WINDOW_SECS: u64 = 60;
const RESTART_LIMIT_PER_WINDOW: usize = 5;

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("Watchdog failed: {err}");
            std::process::exit(1);
        }
    }
}

fn run() -> io::Result<i32> {
    let mut args = std::env::args_os().skip(1);
    let exe = args
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing GUI executable"))?;
    let exe = fs::canonicalize(exe)?;
    let helper = fs::canonicalize(std::env::current_exe()?)?;
    if exe == helper || exe.parent() != helper.parent() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "GUI executable must be beside Watchdog",
        ));
    }
    let Some(_instance) = process::Instance::acquire(&exe)? else {
        return Ok(0);
    };
    let gui_args: Vec<OsString> = args
        .filter(|arg| arg != CHILD_ARG)
        .chain(std::iter::once(OsString::from(CHILD_ARG)))
        .collect();
    let mut logger = Logger::new(exe.parent().unwrap())?;
    logger.write(&format!("{WATCHDOG_EXE} supervisor started"));
    let mut retries = 0;
    // 应用主动请求重启的时间戳，用于滑动窗口限频
    let mut restart_times: Vec<std::time::Instant> = Vec::new();
    loop {
        let status = process::run_child(&exe, &gui_args);
        match status {
            Ok(0) => {
                logger.write("GUI exited normally; supervisor stopping");
                return Ok(0);
            }
            Ok(RESTART_EXIT_CODE) => {
                let now = std::time::Instant::now();
                restart_times
                    .retain(|t| now.duration_since(*t).as_secs() < RESTART_WINDOW_SECS);

                if restart_times.len() >= RESTART_LIMIT_PER_WINDOW {
                    logger.write(&format!(
                        "GUI requested restart {RESTART_LIMIT_PER_WINDOW} times within \
                         {RESTART_WINDOW_SECS}s; restart loop detected, supervisor stopping"
                    ));
                    return Ok(RESTART_EXIT_CODE);
                }

                restart_times.push(now);
                logger.write(&format!(
                    "GUI requested restart ({}/{RESTART_LIMIT_PER_WINDOW} in {}s window)",
                    restart_times.len(),
                    RESTART_WINDOW_SECS
                ));
                retries = 0;
                // 请求重启同样需要间隔，避免零延迟的紧密循环
                std::thread::sleep(Duration::from_secs(RESTART_DELAY_SECS));
            }
            result => {
                let code = match &result {
                    Ok(code) => *code,
                    Err(_) => 1,
                };
                logger.write(&format!("GUI failed: {result:?}"));
                if retries == MAX_RETRIES {
                    logger.write("retry limit reached; supervisor stopping");
                    return Ok(code);
                }
                retries += 1;
                logger.write(&format!(
                    "restarting in {RETRY_DELAY_SECS}s ({retries}/{MAX_RETRIES})"
                ));
                std::thread::sleep(Duration::from_secs(RETRY_DELAY_SECS));
            }
        }
    }
}

struct Logger {
    file: File,
}

impl Logger {
    fn new(exe_dir: &Path) -> io::Result<Self> {
        let path: PathBuf = exe_dir
            .join(".config")
            .join("io.github.clash-verge-buty")
            .join("logs")
            .join("watchdog");
        fs::create_dir_all(&path)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Ok(Self {
            file: OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path.join(format!("{timestamp}-{}.log", std::process::id())))?,
        })
    }

    fn write(&mut self, message: &str) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(self.file, "{timestamp} {message}");
        let _ = self.file.flush();
    }
}
