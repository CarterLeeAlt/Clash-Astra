pub use crate::window::*;

use crate::config::IVerge;
use crate::core::scheduler::{OneShotScheduler, Scheduler};
use crate::utils::init;
use crate::window::show::register_frontend_ready_listener;
use crate::{config::Config, core::*, log_err, utils::server};
use anyhow::Result;
use once_cell::sync::OnceCell;
use serde_yaml::Mapping;
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{App, Manager};

pub static VERSION: OnceCell<String> = OnceCell::new();

pub fn find_unused_port() -> Result<u16> {
    match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => {
            let port = listener.local_addr()?.port();
            Ok(port)
        }
        Err(_) => {
            let port = Config::verge()
                .latest()
                .verge_mixed_port
                .unwrap_or(Config::clash().data().get_mixed_port());
            log::warn!(target: "app", "use default port: {}", port);
            Ok(port)
        }
    }
}

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

/// handle something when start app
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
    // 处理随机端口
    let enable_random_port = Config::verge().latest().enable_random_port.unwrap_or(false);

    let mut port = Config::verge()
        .latest()
        .verge_mixed_port
        .unwrap_or(Config::clash().data().get_mixed_port());

    if enable_random_port {
        port = find_unused_port().unwrap_or(
            Config::verge()
                .latest()
                .verge_mixed_port
                .unwrap_or(Config::clash().data().get_mixed_port()),
        );
    }

    Config::verge().data().patch_config(IVerge {
        verge_mixed_port: Some(port),
        ..IVerge::default()
    });
    let _ = Config::save_verge_snapshot();
    let mut mapping = Mapping::new();
    mapping.insert("mixed-port".into(), port.into());
    Config::clash().data().patch_config(mapping);
    let _ = Config::clash().data().save_config();

    // 启动核心
    log::trace!("init config");
    log_err!(Config::init_config());

    log::trace!("launch core");
    log_err!(CoreManager::global().init());

    // setup a simple http server for singleton
    log::trace!("launch embed server");
    server::embed_server(app.app_handle());

    log::trace!("init system tray");
    log_err!(tray::Tray::update_systray(&app.app_handle()));

    let recovered_visible = crate::utils::watchdog::show_after_recovery();
    let recovered_hidden = crate::utils::watchdog::hide_after_recovery();
    let silent_start = ({ Config::verge().data().enable_silent_start.unwrap_or(false) }
        || recovered_hidden)
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

/// reset system proxy
pub fn resolve_reset() {
    OneShotScheduler::global().cancel_all();
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
