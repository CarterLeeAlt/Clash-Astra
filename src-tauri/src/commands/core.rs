use super::CmdResult;
use crate::{core::CoreManager, wrap_err};

/// 重启受本程序管理的内核。
#[tauri::command]
pub async fn restart_sidecar() -> CmdResult {
    wrap_err!(crate::application::dispatch(async { CoreManager::global().run_core().await }).await)
}
