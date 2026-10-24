use crate::config::Config;
use anyhow::Result;
use std::path::PathBuf;

/// 启动脚本最长等待时间。超过后终止脚本并继续启动应用，
/// 避免脚本挂起导致主窗口永远不出现。
const STARTUP_SCRIPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub fn startup_script() -> Result<()> {
    let path = {
        let verge = Config::verge();
        let verge = verge.latest();
        verge.startup_script.clone().unwrap_or("".to_string())
    };

    if !path.is_empty() {
        let mut shell = "";
        if path.ends_with(".sh") {
            shell = "bash";
        }
        if path.ends_with(".ps1") {
            shell = "powershell";
        }
        if path.ends_with(".bat") {
            shell = "cmd";
        }
        if shell.is_empty() {
            return Err(anyhow::anyhow!("unsupported script: {path}"));
        }
        let current_dir = PathBuf::from(path.clone());
        if !current_dir.exists() {
            return Err(anyhow::anyhow!("script not found: {path}"));
        }

        let mut command = std::process::Command::new(shell);
        if let Some(dir) = current_dir.parent() {
            command.current_dir(dir);
        }
        let mut child = command
            .args(&[path])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        let deadline = std::time::Instant::now() + STARTUP_SCRIPT_TIMEOUT;
        loop {
            match child.try_wait()? {
                Some(_) => break,
                None if std::time::Instant::now() >= deadline => {
                    log::warn!(
                        target: "app",
                        "startup script exceeded {STARTUP_SCRIPT_TIMEOUT:?}, killing it and continuing"
                    );
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                None => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
    }
    Ok(())
}
