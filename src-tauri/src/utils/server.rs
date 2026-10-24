extern crate warp;

use super::resolve;
use crate::config::IVerge;
use anyhow::{bail, Result};
use once_cell::sync::OnceCell;
use port_scanner::local_port_available;
use tauri::AppHandle;
use warp::Filter;

/// 进程级单例句柄。保持存活直到进程退出，Drop 时释放互斥体。
///
/// 用命名互斥体而不是端口判断实例是否已存在：单例端口可能被无关程序占用，
/// 也可能因权限差异导致读不到对方进程的可执行文件路径。命名互斥体由内核
/// 维护，同会话内跨权限级别可见，是 Windows 上判断单例的可靠手段。
#[cfg(target_os = "windows")]
struct SingletonGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(target_os = "windows")]
impl Drop for SingletonGuard {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}

#[cfg(target_os = "windows")]
fn acquire_singleton_mutex() -> Result<bool> {
    // Local\ 前缀限定在当前登录会话内，避免不同用户会话互相阻塞。
    const NAME: &str = "Local\\clash-astra-singleton";
    acquire_singleton_mutex_named(NAME)
}

#[cfg(target_os = "windows")]
fn acquire_singleton_mutex_named(name: &str) -> Result<bool> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = std::ffi::OsStr::new(name)
        .encode_wide()
        .chain(Some(0))
        .collect();

    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle == 0 {
        bail!("failed to create singleton mutex");
    }

    let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if already_exists {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
        return Ok(false);
    }

    static GUARD: OnceCell<SingletonGuard> = OnceCell::new();
    let _ = GUARD.set(SingletonGuard { handle });
    Ok(true)
}

#[cfg(not(target_os = "windows"))]
fn acquire_singleton_mutex() -> Result<bool> {
    Ok(true)
}

/// 单例检测用的本地 HTTP 客户端：严格限时，避免端口被第三方程序
/// 黑洞占用时（建连后不响应）阻塞在 main() 的 block_on 上。
fn singleton_http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(2))
        .build()
}

/// 检查是否已有实例在运行。
///
/// 存在既有实例时返回 Err("app exists")，并尽量唤起它的主窗口。
pub fn check_singleton() -> Result<()> {
    let port = IVerge::get_singleton_port();
    let port_taken = !local_port_available(port);

    // 端口已被占用时，先确认占用者是否为本应用的单例服务。
    // 命中则说明旧实例在运行，直接唤起并退出。
    let is_our_server = port_taken
        && tauri::async_runtime::block_on(async {
            let resp = singleton_http_client()?
                .get(format!("http://127.0.0.1:{port}/commands/ping"))
                .send()
                .await?;
            let text = resp.text().await?;
            Ok::<bool, anyhow::Error>(text.trim() == "ok")
        })
        .unwrap_or(false);

    if is_our_server {
        let _ = tauri::async_runtime::block_on(async {
            let _ = singleton_http_client()?
                .get(format!("http://127.0.0.1:{port}/commands/visible"))
                .send()
                .await;
            Ok::<(), reqwest::Error>(())
        });
        bail!("app exists");
    }

    // 端口未被本应用占用时，以命名互斥体为准判断单例。
    // 这样即便端口被第三方占用、或读不到对方进程路径，也不会启动第二个实例。
    if !acquire_singleton_mutex()? {
        // 另一实例持有互斥体，但它的单例服务未必可用（端口被第三方占用）。
        // 尝试唤起窗口，失败也仍然退出，避免并发写配置。
        let _ = tauri::async_runtime::block_on(async {
            let _ = singleton_http_client()?
                .get(format!("http://127.0.0.1:{port}/commands/visible"))
                .send()
                .await;
            Ok::<(), reqwest::Error>(())
        });
        bail!("app exists");
    }

    if port_taken {
        log::warn!(
            target: "app",
            "singleton port {port} is occupied by another program; \
             the existing window cannot be raised by later launches"
        );
    }

    Ok(())
}

/// The embed server only be used to implement singleton process
/// maybe it can be used as pac server later
pub fn embed_server(app_handle: AppHandle) {
    let port = IVerge::get_singleton_port();

    tauri::async_runtime::spawn(async move {
        // 先显式 bind，端口不可用时只记录日志，不 panic 整个 spawn 任务
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        if let Err(err) = std::net::TcpListener::bind(addr) {
            log::warn!(
                target: "app",
                "singleton listen server cannot bind port {port}: {err}"
            );
            return;
        }

        let ping = warp::path!("commands" / "ping").map(move || "ok");

        let visible = warp::path!("commands" / "visible").map(move || {
            log::trace!("singleton visible command received -> resolve::show_main_window");
            resolve::show_main_window(&app_handle);
            "ok"
        });

        let commands = ping.or(visible);
        warp::serve(commands).run(([127, 0, 0, 1], port)).await;
    });
}

#[cfg(test)]
mod singleton_tests {
    use super::*;

    #[test]
    fn first_acquire_succeeds_and_second_is_rejected() {
        // 使用测试专用名称，避免本机正在运行的应用占用生产互斥体导致测试失败。
        let name = format!(
            "Local\\clash-astra-singleton-test-{}",
            std::process::id()
        );

        // 首次获取成功；同一进程内再次获取时互斥体已存在，应返回 false
        assert!(acquire_singleton_mutex_named(&name).unwrap());
        assert!(!acquire_singleton_mutex_named(&name).unwrap());
    }
}
