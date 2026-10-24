use super::CmdResult;
use crate::{
    ret_err,
    utils::{dirs, resolve},
    wrap_err,
};
use serde_yaml::Mapping;
use sysproxy::Sysproxy;
use tauri::{api, Manager};

/// get the system proxy
#[tauri::command]
pub fn get_sys_proxy() -> CmdResult<Mapping> {
    let current = wrap_err!(Sysproxy::get_system_proxy())?;

    let mut map = Mapping::new();
    map.insert("enable".into(), current.enable.into());
    map.insert(
        "server".into(),
        format!("{}:{}", current.host, current.port).into(),
    );
    map.insert("bypass".into(), current.bypass.into());

    Ok(map)
}

#[tauri::command]
pub fn open_app_dir() -> CmdResult<()> {
    let app_dir = wrap_err!(dirs::app_home_dir())?;
    wrap_err!(open::that(app_dir))
}

#[tauri::command]
pub fn open_core_dir() -> CmdResult<()> {
    let core_dir = wrap_err!(tauri::utils::platform::current_exe())?;
    let core_dir = core_dir.parent().ok_or("failed to get core dir")?;
    wrap_err!(open::that(core_dir))
}

#[tauri::command]
pub fn open_logs_dir() -> CmdResult<()> {
    let log_dir = wrap_err!(crate::utils::log_export::prepare_redacted_logs_export())?;
    wrap_err!(open::that(log_dir))
}

#[tauri::command]
pub fn open_web_url(url: String) -> CmdResult<()> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();

    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        ret_err!("Only http(s) URLs are allowed");
    }

    wrap_err!(open::that(url))
}

#[tauri::command]
pub fn get_app_dir() -> CmdResult<String> {
    let app_home_dir = wrap_err!(dirs::app_home_dir())?
        .to_string_lossy()
        .to_string();
    Ok(app_home_dir)
}

#[tauri::command]
pub fn copy_icon_file(path: String, name: String) -> CmdResult<String> {
    let file_path = std::path::Path::new(&path);
    let icon_dir = wrap_err!(dirs::app_home_dir())?.join("icons");

    let file_name = std::path::Path::new(&name)
        .file_name()
        .ok_or("invalid file name")?
        .to_os_string();
    if file_name.to_string_lossy().is_empty() {
        ret_err!("invalid file name");
    }
    if !icon_dir.exists() {
        let _ = std::fs::create_dir_all(&icon_dir);
    }
    let dest_path = icon_dir.join(file_name);

    if file_path.exists() {
        match std::fs::copy(file_path, &dest_path) {
            Ok(_) => Ok(dest_path.to_string_lossy().to_string()),
            Err(err) => Err(err.to_string()),
        }
    } else {
        return Err("file not found".to_string());
    }
}

#[tauri::command]
pub fn frontend_heartbeat() -> CmdResult<()> {
    resolve::record_frontend_heartbeat();
    Ok(())
}

#[tauri::command]
pub fn report_frontend_error(message: String, stack: Option<String>) -> CmdResult<()> {
    resolve::record_frontend_error(message, stack);
    Ok(())
}

#[tauri::command]
pub fn exit_app(app_handle: tauri::AppHandle) {
    shutdown_app(app_handle, 0);
}

pub fn restart_app(app_handle: tauri::AppHandle) {
    #[cfg(target_os = "windows")]
    if crate::utils::watchdog::is_supervised() {
        shutdown_app(app_handle, crate::watchdog_protocol::RESTART_EXIT_CODE);
        return;
    }

    log::info!(target: "app", "restart_app without supervisor, restarting directly");
    resolve::set_app_quitting(true);
    let _ = resolve::save_window_size_position(&app_handle, true);
    resolve::resolve_reset();
    api::process::kill_children();
    api::process::restart(&app_handle.env());
}

fn shutdown_app(app_handle: tauri::AppHandle, exit_code: i32) {
    log::info!(target: "app", "shutdown_app called, exit_code={exit_code}");
    resolve::set_app_quitting(true);
    let _ = resolve::save_window_size_position(&app_handle, true);
    resolve::resolve_reset();
    api::process::kill_children();
    app_handle.exit(exit_code);
    std::process::exit(exit_code);
}
