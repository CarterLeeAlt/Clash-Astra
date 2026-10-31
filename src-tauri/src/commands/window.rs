use super::CmdResult;
use crate::{utils::resolve, wrap_err};

#[tauri::command]
pub async fn set_window_size_locked(app_handle: tauri::AppHandle, locked: bool) -> CmdResult {
    wrap_err!(crate::application::window::set_size_locked(app_handle, locked).await)
}

#[tauri::command(async)]
pub fn get_window_style_config() -> CmdResult<resolve::WindowStyleConfig> {
    Ok(resolve::get_window_style_config())
}
