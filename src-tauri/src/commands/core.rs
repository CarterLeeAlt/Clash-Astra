use super::CmdResult;
use crate::{core::CoreManager, wrap_err};

#[tauri::command]
pub async fn change_clash_core(clash_core: Option<String>) -> CmdResult {
    let clash_core = clash_core.ok_or("Mihomo core is null")?;
    wrap_err!(
        crate::application::configuration::patch_verge(crate::config::IVerge {
            clash_core: Some(clash_core),
            ..Default::default()
        })
        .await
    )
}

/// 重启受本程序管理的内核。
#[tauri::command]
pub async fn restart_sidecar() -> CmdResult {
    wrap_err!(crate::application::dispatch(async { CoreManager::global().run_core().await }).await)
}

#[tauri::command]
pub async fn upgrade_core() -> CmdResult<bool> {
    let updated = wrap_err!(
        crate::application::dispatch(async { CoreManager::global().upgrade_core().await }).await
    )?;
    crate::core::handle::Handle::refresh_clash();
    Ok(updated)
}
