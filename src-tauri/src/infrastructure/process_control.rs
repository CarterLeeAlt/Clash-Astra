//! Bounded child-process execution shared by the GUI and its Windows helpers.
use anyhow::{Context, Result};
use std::{
    io::Read,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

pub fn wait(child: &mut Child, deadline: Instant) -> Result<std::process::ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            stop(child, Instant::now() + Duration::from_millis(500))?;
            anyhow::bail!("child process exceeded its deadline");
        }
        std::thread::sleep(
            Duration::from_millis(20).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

pub fn stop(child: &mut Child, deadline: Instant) -> Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    if let Err(error) = child.kill() {
        if child.try_wait()?.is_none() {
            return Err(error).context("failed to stop owned child process");
        }
    }
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "owned child process did not exit before deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn output(command: &mut Command, timeout: Duration) -> Result<Output> {
    check_helper_deadline()?;
    let timeout = helper_remaining(timeout)?;
    anyhow::ensure!(!timeout.is_zero(), "process deadline expired before spawn");
    let deadline = Instant::now() + timeout.min(Duration::from_secs(60));
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let (tx, rx) = std::sync::mpsc::channel();
    for (stderr, stream) in [
        (
            false,
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
        ),
        (
            true,
            Box::new(child.stderr.take().unwrap()) as Box<dyn Read + Send>,
        ),
    ] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stream
                .take(1024 * 1024)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = tx.send((stderr, result));
        });
    }
    let status = wait(&mut child, deadline)?;
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    for _ in 0..2 {
        let (is_stderr, result) = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .context("child output did not close before deadline")?;
        if is_stderr {
            stderr = result?;
        } else {
            stdout = result?;
        }
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Helpers reject an expired approval before any host mutation, including late UAC approval.
pub fn check_helper_deadline() -> Result<()> {
    helper_remaining(Duration::from_secs(60)).map(|_| ())
}

fn helper_remaining(limit: Duration) -> Result<Duration> {
    if let Some(index) = std::env::args().position(|arg| arg == "--deadline-unix-ms") {
        let deadline: u128 = std::env::args()
            .nth(index + 1)
            .context("missing helper deadline")?
            .parse()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis();
        return deadline_remaining(limit, deadline, now);
    }
    Ok(limit.min(Duration::from_secs(60)))
}

fn deadline_remaining(limit: Duration, deadline: u128, now: u128) -> Result<Duration> {
    anyhow::ensure!(now < deadline, "helper approval/execution deadline expired");
    Ok(limit.min(Duration::from_millis((deadline - now).min(60000) as u64)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_approval_cannot_receive_a_fresh_budget() {
        assert!(deadline_remaining(Duration::from_secs(60), 100, 100).is_err());
        assert!(deadline_remaining(Duration::from_secs(60), 100, 101).is_err());
        assert_eq!(
            deadline_remaining(Duration::from_secs(60), 100, 90).unwrap(),
            Duration::from_millis(10)
        );
        assert_eq!(
            deadline_remaining(Duration::from_secs(120), 120000, 0).unwrap(),
            Duration::from_secs(60)
        );
    }
    #[test]
    fn hung_child_is_stopped_and_nonzero_status_is_preserved() {
        let mut command = Command::new("cmd.exe");
        command.args(["/C", "exit 7"]);
        assert_eq!(
            output(&mut command, Duration::from_secs(2))
                .unwrap()
                .status
                .code(),
            Some(7)
        );
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 20",
        ]);
        let started = Instant::now();
        assert!(output(&mut command, Duration::from_millis(150)).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
