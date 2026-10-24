use crate::config::Config;
use crate::utils::{dirs, redact::redact_log_text};
use anyhow::Result;
use chrono::{Duration, Local};
use log::LevelFilter;
use log4rs::append::console::ConsoleAppender;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Logger, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::encode::{self, writer::simple::SimpleWriter};
use std::fmt;
use std::fs::{self, DirEntry};
use std::path::PathBuf;
use std::time::SystemTime;

struct RedactingEncoder {
    inner: PatternEncoder,
}

impl RedactingEncoder {
    fn new(pattern: &str) -> Self {
        Self {
            inner: PatternEncoder::new(pattern),
        }
    }
}

impl fmt::Debug for RedactingEncoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RedactingEncoder").finish_non_exhaustive()
    }
}

impl encode::Encode for RedactingEncoder {
    fn encode(&self, w: &mut dyn encode::Write, record: &log::Record) -> anyhow::Result<()> {
        let mut bytes = Vec::new();
        {
            let mut writer = SimpleWriter(&mut bytes);
            self.inner.encode(&mut writer, record)?;
        }

        let text = String::from_utf8_lossy(&bytes);
        let redacted = redact_log_text(&text);
        w.write_all(redacted.as_bytes())?;
        Ok(())
    }
}

/// initialize this instance's log file
pub(crate) fn init_log() -> Result<()> {
    let log_dir = dirs::app_logs_dir()?;

    let log_level = Config::verge().data().get_log_level();
    if log_level == LevelFilter::Off {
        return Ok(());
    }

    let local_time = Local::now().format("%Y-%m-%d-%H%M").to_string();
    let log_file = format!("{}.log", local_time);
    let log_file = log_dir.join(log_file);

    let log_pattern = match log_level {
        LevelFilter::Trace => "{d(%Y-%m-%d %H:%M:%S)} {l} [{M}] - {m}{n}",
        _ => "{d(%Y-%m-%d %H:%M:%S)} {l} - {m}{n}",
    };

    let stdout_encode = Box::new(PatternEncoder::new(log_pattern));
    let file_encode = Box::new(RedactingEncoder::new(log_pattern));

    let stdout = ConsoleAppender::builder().encoder(stdout_encode).build();
    let tofile = FileAppender::builder()
        .encoder(file_encode)
        .build(log_file)?;

    let mut logger_builder = Logger::builder();
    let mut root_builder = Root::builder();

    let log_more = log_level == LevelFilter::Trace || log_level == LevelFilter::Debug;
    let dependency_log_level = LevelFilter::Warn;

    #[cfg(feature = "verge-dev")]
    {
        logger_builder = logger_builder.appenders(["file", "stdout"]);
        if log_more {
            root_builder = root_builder.appenders(["file", "stdout"]);
        } else {
            root_builder = root_builder.appenders(["stdout"]);
        }
    }
    #[cfg(not(feature = "verge-dev"))]
    {
        logger_builder = logger_builder.appenders(["file"]);
        if log_more {
            root_builder = root_builder.appenders(["file"]);
        }
    }

    let (config, _) = log4rs::config::Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .appender(Appender::builder().build("file", Box::new(tofile)))
        .logger(logger_builder.additive(false).build("app", log_level))
        .build_lossy(root_builder.build(dependency_log_level));

    log4rs::init_config(config)?;

    Ok(())
}

/// 删除log文件
pub fn delete_log() -> Result<()> {
    let auto_log_clean = Config::verge().data().auto_log_clean.unwrap_or(1);

    let day = match auto_log_clean {
        0 => return Ok(()),
        1 => 7,
        2 => 30,
        3 => 90,
        _ => 7,
    };

    log::debug!(target: "app", "try to delete log files, day: {day}");
    let cutoff = SystemTime::from(Local::now() - Duration::days(day));
    fn clean_log_files_in_dir(dir: &PathBuf, cutoff: SystemTime) -> Result<usize> {
        if !dir.exists() {
            return Ok(0);
        }

        let mut deleted_count = 0usize;

        let mut process_file = |file: DirEntry| -> Result<()> {
            let file_type = file.file_type()?;
            if !file_type.is_file() {
                return Ok(());
            }

            let file_name = file.file_name();
            let file_name = file_name.to_str().unwrap_or_default();

            if file_name.ends_with(".log") {
                let metadata = fs::metadata(file.path())?;
                let file_time = metadata.modified().or_else(|_| metadata.created());
                let file_time = match file_time {
                    Ok(time) => time,
                    Err(err) => {
                        log::warn!(
                            target: "app",
                            "skip log file due to unreadable timestamp: {file_name}, {err}"
                        );
                        return Ok(());
                    }
                };

                if file_time < cutoff {
                    match fs::remove_file(file.path()) {
                        Ok(_) => {
                            deleted_count += 1;
                            log::info!(target: "app", "delete log file: {file_name}");
                        }
                        Err(err) => {
                            log::warn!(target: "app", "failed to delete log file: {file_name}, {err}");
                        }
                    }
                }
            }
            Ok(())
        };

        for file in fs::read_dir(dir)?.flatten() {
            let _ = process_file(file);
        }

        Ok(deleted_count)
    }

    let mut deleted_count = 0usize;
    let app_log_dir = dirs::app_logs_dir()?;
    deleted_count += clean_log_files_in_dir(&app_log_dir, cutoff)?;

    let service_log_dir = dirs::service_logs_dir()?;
    deleted_count += clean_log_files_in_dir(&service_log_dir, cutoff)?;

    log::info!(target: "app", "log clean finished, deleted {deleted_count} files");
    Ok(())
}
