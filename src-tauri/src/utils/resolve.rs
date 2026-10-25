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

/// 前端心跳文件：由主线程上的 frontend_heartbeat 命令刷新，
/// 看门狗通过它判断 WebView 是否还在运行（主线程死锁时也会停止刷新）。
pub fn frontend_heartbeat_path() -> Result<PathBuf> {
    Ok(crate::utils::dirs::app_home_dir()?
        .join("runtime")
        .join("gui-heartbeat"))
}

pub fn touch_frontend_heartbeat() -> Result<()> {
    let path = frontend_heartbeat_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, b"")?;
    Ok(())
}

pub fn reset_frontend_heartbeat() -> Result<()> {
    let path = frontend_heartbeat_path()?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// handle something when start app
pub fn resolve_setup(app: &mut App) {
    let version = app.package_info().version.to_string();
    handle::Handle::global().init(app.app_handle());
    VERSION.get_or_init(|| version.clone());

    log_err!(sysopt::Sysopt::global().recover_pending_sysproxy());
    log_err!(init::init_resources());
    #[cfg(target_os = "windows")]
    log_err!(crate::core::win_firewall::reconcile_lan_firewall_on_startup());
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
    let _ = reset_frontend_heartbeat();
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
    let _ = reset_frontend_heartbeat();
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
