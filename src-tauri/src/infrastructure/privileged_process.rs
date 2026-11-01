use anyhow::{Context, Result};
use std::{
    os::windows::{
        ffi::OsStrExt,
        process::{CommandExt, ExitStatusExt},
    },
    path::Path,
    process::{Command, ExitStatus},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::Threading::{GetExitCodeProcess, TerminateProcess, WaitForSingleObject},
    UI::Shell::{ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW},
};

/// The helper also checks this absolute deadline before host writes, so a late
/// UAC approval cannot execute an operation whose caller has already timed out.
pub fn run(path: &Path, args: &[&str], elevated: bool) -> Result<ExitStatus> {
    anyhow::ensure!(elevated || !crate::application::budget::is_recovery(),
        "automatic compensation requires administrator privileges; a second UAC prompt was not issued");
    let timeout = crate::application::budget::remaining(Duration::from_secs(45))?;
    let deadline = Instant::now() + timeout;
    let unix_deadline = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?
        + timeout)
        .as_millis()
        .to_string();
    let mut arguments = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    arguments.extend(["--deadline-unix-ms".into(), unix_deadline]);
    if elevated {
        let mut command = Command::new(path);
        command.creation_flags(0x08000000).args(arguments);
        return Ok(super::process_control::output(&mut command, timeout)?.status);
    }
    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let params = arguments
        .iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<ExitStatus> {
            let verb = "runas\0".encode_utf16().collect::<Vec<_>>();
            let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
            info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
            info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI;
            info.lpVerb = verb.as_ptr();
            info.lpFile = path.as_ptr();
            info.lpParameters = params.as_ptr();
            info.nShow = 0;
            unsafe {
                anyhow::ensure!(
                    ShellExecuteExW(&mut info) != 0,
                    "administrator approval failed: {}",
                    std::io::Error::last_os_error()
                );
                anyhow::ensure!(
                    info.hProcess != 0,
                    "elevated helper returned no process handle"
                );
                struct Handle(isize);
                impl Drop for Handle {
                    fn drop(&mut self) {
                        unsafe {
                            CloseHandle(self.0);
                        }
                    }
                }
                let handle = Handle(info.hProcess);
                let left = deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis() as u32;
                let state = WaitForSingleObject(handle.0, left);
                if state != 0 {
                    let stopped = TerminateProcess(handle.0, 1);
                    anyhow::ensure!(
                        stopped != 0,
                        "helper timed out and termination could not be confirmed: {}",
                        std::io::Error::last_os_error()
                    );
                    anyhow::bail!("administrator approval/helper execution timed out");
                }
                let mut code = 0;
                anyhow::ensure!(
                    GetExitCodeProcess(handle.0, &mut code) != 0,
                    "could not read helper exit status"
                );
                Ok(ExitStatus::from_raw(code))
            }
        })();
        let _ = tx.send(result);
    });
    rx.recv_timeout(timeout)
        .context("administrator approval/helper execution deadline expired")?
}

fn quote(arg: &str) -> String {
    let mut quoted = String::from("\"");
    let mut slashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        quoted.extend(std::iter::repeat_n(
            '\\',
            if c == '"' { slashes * 2 + 1 } else { slashes },
        ));
        slashes = 0;
        quoted.push(c);
    }
    quoted.extend(std::iter::repeat_n('\\', slashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_arguments_keep_spaces_quotes_and_trailing_slashes() {
        assert_eq!(quote("a b"), "\"a b\"");
        assert_eq!(quote("x\\"), "\"x\\\\\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
    }
}
