use super::CmdResult;
use crate::{config::*, utils::resolve, wrap_err};
use tauri::Manager;

#[tauri::command]
pub async fn set_window_size_locked(app_handle: tauri::AppHandle, locked: bool) -> CmdResult {
    let window = app_handle
        .get_window("main")
        .ok_or_else(|| "failed to get window".to_string())?;

    if locked {
        let mut is_maximized = wrap_err!(window.is_maximized())?;
        if is_maximized {
            wrap_err!(window.unmaximize())?;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            is_maximized = wrap_err!(window.is_maximized())?;
            if is_maximized {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                is_maximized = wrap_err!(window.is_maximized())?;
            }
        }

        if is_maximized {
            log::warn!(target: "app", "window is still maximized after unmaximize, skip saving size/position before locking");
        } else {
            wrap_err!(resolve::save_window_size_position(&app_handle, false))?;
        }
        wrap_err!(window.set_resizable(false))?;
        wrap_err!(window.set_maximizable(false))?;

        Config::verge().draft().patch_config(IVerge {
            window_is_maximized: Some(false),
            window_size_locked: Some(true),
            ..IVerge::default()
        });
        Config::verge().apply();
        wrap_err!(Config::save_verge_snapshot())?;
    } else {
        wrap_err!(window.set_resizable(true))?;
        wrap_err!(window.set_maximizable(true))?;

        Config::verge().draft().patch_config(IVerge {
            window_size_locked: Some(false),
            ..IVerge::default()
        });
        Config::verge().apply();
        wrap_err!(Config::save_verge_snapshot())?;
    }

    Ok(())
}

#[tauri::command]
pub fn get_window_style_config() -> CmdResult<resolve::WindowStyleConfig> {
    Ok(resolve::get_window_style_config())
}
