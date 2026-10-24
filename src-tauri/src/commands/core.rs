use super::CmdResult;
use crate::{core::CoreManager, wrap_err};

#[tauri::command]
pub async fn change_clash_core(clash_core: Option<String>) -> CmdResult {
    wrap_err!(CoreManager::global().change_core(clash_core).await)
}

/// restart the sidecar
#[tauri::command]
pub async fn restart_sidecar() -> CmdResult {
    wrap_err!(CoreManager::global().run_core().await)
}

#[tauri::command]
pub async fn upgrade_core() -> CmdResult<bool> {
    wrap_err!(CoreManager::global().upgrade_core().await)
}
