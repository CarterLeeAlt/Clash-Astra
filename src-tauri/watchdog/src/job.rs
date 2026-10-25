use std::io;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
};

/// 把 GUI 及其子进程（mihomo、WebView2）放进同一个 Job。
///
/// Job 句柄关闭时内核会结束组内所有进程，避免 GUI 被看门狗结束后
/// mihomo/WebView2 残留并占住端口，导致下一次启动失败。
pub struct Job {
    handle: HANDLE,
}

impl Job {
    pub fn create() -> io::Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle == 0 {
                return Err(io::Error::last_os_error());
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                let err = io::Error::last_os_error();
                CloseHandle(handle);
                return Err(err);
            }

            Ok(Self { handle })
        }
    }

    pub fn assign(&self, process: HANDLE) -> io::Result<()> {
        unsafe {
            if AssignProcessToJobObject(self.handle, process) == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    /// 结束 Job 内的全部进程；空 Job 调用也安全。
    pub fn terminate(&self) -> io::Result<()> {
        unsafe {
            if TerminateJobObject(self.handle, 1) == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}
