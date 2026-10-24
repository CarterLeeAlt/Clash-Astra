use crate::utils::{dirs, redact::redact_log_text};
use chrono::Local;
use std::backtrace::Backtrace;
use std::fs;
use std::panic;

/// Initialize panic hook to persist crash diagnostics before the process exits.
pub fn init_panic_hook() {
    panic::set_hook(Box::new(|info| {
        let timestamp = Local::now();
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|value| value.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "<unknown>".to_string());
        let backtrace = Backtrace::force_capture();
        let content = format!(
            "timestamp: {}\nthread: {}\npayload: {}\nlocation: {}\n\nbacktrace:\n{}\n",
            timestamp.to_rfc3339(),
            thread_name,
            payload,
            location,
            backtrace
        );

        match dirs::app_logs_dir() {
            Ok(log_dir) => {
                let file_name = format!("crash-{}.log", timestamp.format("%Y-%m-%d-%H%M%S"));
                let content = redact_log_text(&content);
                if let Err(err) = fs::write(log_dir.join(file_name), &content) {
                    log::error!(target: "app", "failed to write panic crash log: {err}");
                }
            }
            Err(err) => {
                log::error!(target: "app", "failed to resolve panic crash log dir: {err}");
            }
        }

        log::error!(
            target: "app",
            "panic captured, thread={}, payload={}, location={}",
            thread_name,
            payload,
            location
        );
    }));
}
