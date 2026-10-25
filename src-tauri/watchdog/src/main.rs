#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod job;
mod process;
#[path = "../../src/watchdog_protocol.rs"]
mod protocol;
mod runtime_files;
mod window;

use job::Job;
use process::Outcome;
use protocol::{
    CHILD_ARG, RECOVERED_HIDDEN_ARG, RECOVERED_VISIBLE_ARG, RESTART_EXIT_CODE, WATCHDOG_EXE,
};
use runtime_files::RuntimeFiles;
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_RETRIES: usize = 3;
const RETRY_DELAY_SECS: u64 = 10;
/// 应用请求重启后的等待时间，避免立即重新拉起造成紧密循环。
const RESTART_DELAY_SECS: u64 = 2;
/// 单位时间窗内允许的「应用请求重启」次数上限。
const RESTART_WINDOW_SECS: u64 = 60;
const RESTART_LIMIT_PER_WINDOW: usize = 5;
/// 卡死重启的窗口和上限：达到上限后本次运行不再做卡死检测（只等它退出），
/// 避免启动即卡死或误判时界面反复消失。
const HANG_WINDOW_SECS: u64 = 600;
const MAX_HANG_RESTARTS: usize = 3;
/// 运行超过这个时间视为稳定，清零崩溃重试计数。
const STABLE_RUN_SECS: u64 = 60;

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

    let base_args: Vec<OsString> = args
        .filter(|arg| {
            arg != CHILD_ARG && arg != RECOVERED_VISIBLE_ARG && arg != RECOVERED_HIDDEN_ARG
        })
        .collect();
    let exe_dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let mut logger = Logger::new(exe_dir)?;
    logger.write(&format!("{WATCHDOG_EXE} supervisor started"));

    let job = match Job::create() {
        Ok(job) => Some(job),
        Err(err) => {
            logger.write(&format!(
                "failed to create job object: {err}; child cleanup will use taskkill"
            ));
            None
        }
    };
    let files = RuntimeFiles::new(exe_dir);

    let mut retries = 0usize;
    let mut restart_times: Vec<Instant> = Vec::new();
    let mut hang_times: Vec<Instant> = Vec::new();
    let mut restore_visible = false;
    let mut restore_hidden = false;

    loop {
        let mut gui_args = base_args.clone();
        if restore_visible {
            gui_args.push(OsString::from(RECOVERED_VISIBLE_ARG));
        } else if restore_hidden {
            gui_args.push(OsString::from(RECOVERED_HIDDEN_ARG));
        }
        gui_args.push(OsString::from(CHILD_ARG));

        let now = Instant::now();
        hang_times.retain(|time| now.duration_since(*time).as_secs() < HANG_WINDOW_SECS);
        let hang_detection = hang_times.len() < MAX_HANG_RESTARTS;
        if !hang_detection {
            logger.write(&format!(
                "GUI hung {MAX_HANG_RESTARTS} times within {HANG_WINDOW_SECS}s; \
                 hang detection disabled for this run"
            ));
        }

        let outcome = process::supervise_child(
            &exe,
            &gui_args,
            job.as_ref(),
            &files,
            hang_detection,
            &mut |message| logger.write(message),
        );
        let uptime = outcome.uptime();

        if let Some(job) = &job {
            if let Err(err) = job.terminate() {
                logger.write(&format!("failed to terminate job after GUI exit: {err}"));
            }
        }

        if uptime >= Duration::from_secs(STABLE_RUN_SECS) {
            retries = 0;
        }

        match outcome {
            Outcome::Exited {
                status: Ok(status), ..
            } if status.code() == Some(0) => {
                logger.write("GUI exited normally; supervisor stopping");
                return Ok(0);
            }
            Outcome::Exited {
                status: Ok(status), ..
            } if status.code() == Some(RESTART_EXIT_CODE) => {
                let now = Instant::now();
                restart_times
                    .retain(|time| now.duration_since(*time).as_secs() < RESTART_WINDOW_SECS);

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
                restore_visible = false;
                restore_hidden = false;
                std::thread::sleep(Duration::from_secs(RESTART_DELAY_SECS));
            }
            Outcome::Exited { status, .. } => {
                let code = status
                    .as_ref()
                    .ok()
                    .and_then(|status| status.code())
                    .unwrap_or(1);
                logger.write(&format!("GUI failed: {status:?}"));
                if retries >= MAX_RETRIES {
                    logger.write("retry limit reached; supervisor stopping");
                    return Ok(code);
                }
                retries += 1;
                logger.write(&format!(
                    "restarting in {RETRY_DELAY_SECS}s ({retries}/{MAX_RETRIES})"
                ));
                std::thread::sleep(Duration::from_secs(RETRY_DELAY_SECS));
            }
            Outcome::Hung { visible, .. } => {
                hang_times.push(Instant::now());
                logger.write(&format!(
                    "GUI hang detected; restarting ({}/{MAX_HANG_RESTARTS} in {}s window), \
                     restore_visible={visible}",
                    hang_times.len(),
                    HANG_WINDOW_SECS
                ));
                retries = 0;
                restore_visible = visible;
                restore_hidden = !visible;
                std::thread::sleep(Duration::from_secs(RESTART_DELAY_SECS));
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
            .join("io.github.clash-astra")
            .join("logs")
            .join("watchdog");
        fs::create_dir_all(&path)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let file_path = path.join(format!("{timestamp}-{}.log", std::process::id()));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&file_path)
            .or_else(|_| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&file_path)
            })?;
        Ok(Self { file })
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
