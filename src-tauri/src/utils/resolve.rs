pub use crate::window::*;

use crate::bootstrap as init;
use crate::core::scheduler::{OneShotScheduler, Scheduler};
use crate::window::show::register_frontend_ready_listener;
use crate::{config::Config, core::*, log_err, utils::server};
use anyhow::Result;
use once_cell::sync::OnceCell;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{App, Manager};

pub static VERSION: OnceCell<String> = OnceCell::new();

fn watchdog_runtime_file(name: &str) -> Result<PathBuf> {
    Ok(crate::utils::dirs::app_home_dir()?
        .join("runtime")
        .join(name))
}

/// 写入当前 Unix 毫秒时间戳。看门狗读内容判断，不看 mtime：
/// 反复把空文件截断为空时 NTFS 不更新 LastWriteTime。
fn write_watchdog_timestamp(name: &str) -> Result<()> {
    let path = watchdog_runtime_file(name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, current_time_millis().to_string())?;
    Ok(())
}

fn remove_watchdog_file(name: &str) -> Result<()> {
    match fs::remove_file(watchdog_runtime_file(name)?) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// 前端心跳：由主线程上的 frontend_heartbeat 命令刷新，
/// 看门狗通过它判断 WebView 是否还在运行（主线程死锁时也会停止刷新）。
pub fn touch_frontend_heartbeat() -> Result<()> {
    write_watchdog_timestamp(crate::watchdog_protocol::HEARTBEAT_FILE)
}

/// 告诉看门狗应用开始退出：退出清理在主线程上可能阻塞几十秒，
/// 不能被当成卡死而结束进程再重新拉起。
pub fn mark_quitting_for_watchdog() {
    if let Err(err) = write_watchdog_timestamp(crate::watchdog_protocol::QUITTING_FILE) {
        log::warn!(target: "app", "failed to write watchdog quitting marker: {err}");
    }
}

/// 启动时清掉上次运行留下的心跳和退出标记。
pub fn reset_watchdog_files() {
    for name in [
        crate::watchdog_protocol::HEARTBEAT_FILE,
        crate::watchdog_protocol::QUITTING_FILE,
    ] {
        if let Err(err) = remove_watchdog_file(name) {
            log::warn!(target: "app", "failed to reset watchdog file {name}: {err}");
        }
    }
}

/// 按依赖顺序启动配置、内核、单实例通信、托盘、窗口和后台任务。
pub fn resolve_setup(app: &mut App) {
    let version = app.package_info().version.to_string();
    handle::Handle::global().init(app.app_handle());
    VERSION.get_or_init(|| version.clone());

    log_err!(sysopt::Sysopt::global().recover_pending_sysproxy());
    log_err!(init::init_resources());
    // 防火墙 helper 是外部进程且没有超时，与建窗无关，不能阻塞 setup（主线程）
    #[cfg(target_os = "windows")]
    std::thread::spawn(|| {
        log_err!(crate::core::win_firewall::reconcile_lan_firewall_on_startup());
    });
    log_err!(init::startup_script());
    match crate::application::configuration::initialize() {
        Ok(()) => log_err!(CoreManager::global().init()),
        Err(error) => {
            log::error!(target: "app", "configuration initialization failed; open the dashboard to repair it: {error:#}")
        }
    }

    // 启动单实例唤起服务器。
    log::trace!("launch embed server");
    server::embed_server(app.app_handle());

    log::trace!("init system tray");
    log_err!(tray::Tray::update_systray(&app.app_handle()));

    let recovered_visible = crate::utils::watchdog::show_after_recovery();
    let recovered_hidden = crate::utils::watchdog::hide_after_recovery();
    let silent_start = ({
        Config::clash_settings()
            .enable_silent_start
            .unwrap_or(false)
    } || recovered_hidden)
        && !recovered_visible;
    let show_when_ready = !silent_start;
    reset_watchdog_files();
    log::trace!(
        "resolve_setup pre-create main window, silent_start={}, recovered_visible={}, recovered_hidden={}, show_when_ready={}",
        silent_start,
        recovered_visible,
        recovered_hidden,
        show_when_ready
    );
    register_frontend_ready_listener(&app.app_handle(), show_when_ready);
    create_window(&app.app_handle(), show_when_ready);

    log_err!(sysopt::Sysopt::global().init_launch());
    #[cfg(target_os = "windows")]
    log_err!(sysopt::Sysopt::global().update_launch());

    log_err!(handle::Handle::update_systray_part());
    log_err!(hotkey::Hotkey::global().init(app.app_handle()));
    log_err!(scheduler::init());
}

/// 关闭业务入口并等待在途修改完成，然后停止任务、恢复代理并停止内核。
pub fn resolve_reset() {
    let _operations = crate::application::shutdown_guard();
    OneShotScheduler::global().shutdown();
    log_err!(Scheduler::global().shutdown());
    log_err!(sysopt::Sysopt::global().reset_sysproxy());
    log_err!(CoreManager::global().stop_core());
    #[cfg(target_os = "windows")]
    log_err!(tauri::async_runtime::block_on(async {
        match tokio::time::timeout(
            Duration::from_secs(5),
            crate::core::win_service::stop_service_if_idle(),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                log::error!(target: "app", "stop_service_if_idle timed out after 5s; continue shutdown");
                Ok(())
            }
        }
    }));
}
