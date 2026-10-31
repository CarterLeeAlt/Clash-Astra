use super::CmdResult;
use crate::{core::win_service, wrap_err};

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
    wrap_err!(crate::application::service::install().await)
}

#[tauri::command]
pub async fn uninstall_service() -> CmdResult {
    wrap_err!(crate::application::service::uninstall().await)
}
