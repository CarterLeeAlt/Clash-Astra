use anyhow::{Context, Result};
use std::{ffi::OsStr, process::Command};

fn run(mut command: Command) -> Result<()> {
    let status = super::external_process::configure_external(&mut command)
        .status()
        .context("failed to launch external application outside the supervisor job")?;
    anyhow::ensure!(
        status.success(),
        "external application launch failed: {status}"
    );
    Ok(())
}

/// 使用系统关联程序打开目录或网址，外部进程不随主程序退出。
pub fn open(path: impl AsRef<OsStr>) -> Result<()> {
    let command = open::commands(path)
        .into_iter()
        .next()
        .context("no desktop open command")?;
    run(command)
}

/// 优先交给 VS Code，失败后由系统文件关联处理。
pub fn open_file(path: impl AsRef<OsStr>) -> Result<()> {
    let path = path.as_ref();
    run(open::with_command(path, "code")).or_else(|_| open(path))
}
