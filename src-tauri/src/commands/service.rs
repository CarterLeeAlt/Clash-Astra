use super::CmdResult;
use crate::{config::*, core::win_service, wrap_err};

#[tauri::command]
pub async fn check_service() -> CmdResult<win_service::ServiceStatus> {
    wrap_err!(win_service::check_service().await)
}

#[tauri::command(async)]
pub fn is_elevated() -> CmdResult<bool> {
    wrap_err!(win_service::is_current_process_elevated())
}

#[tauri::command]
pub async fn install_service() -> CmdResult {
    wrap_err!(win_service::install_service().await)
}

#[tauri::command]
pub async fn uninstall_service() -> CmdResult {
    let tun_mode_enabled = {
        let verge_config = Config::verge();
        let verge = verge_config.latest();
        verge.enable_tun_mode.unwrap_or(false)
    };
    if tun_mode_enabled {
        return Err("Disable TUN mode before uninstalling the service.".to_string());
    }
    if let Err(err) = win_service::stop_core_by_service().await {
        log::warn!(target: "app", "stop_core_by_service before uninstall failed: {err}");
    }

    match win_service::uninstall_service().await {
        Ok(_) => {
            Config::verge().draft().patch_config(IVerge {
                enable_service_mode: Some(false),
                enable_tun_mode: Some(false),
                ..IVerge::default()
            });
            Config::verge().apply();
            wrap_err!(Config::save_verge_snapshot())
        }
        Err(err) => Err(err.to_string()),
    }
}
