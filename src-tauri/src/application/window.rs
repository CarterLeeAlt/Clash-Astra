use anyhow::{Context, Result};
use tauri::Manager;

/// 原生窗口约束与持久化设置作为一个业务操作执行，失败时恢复窗口约束。
pub async fn set_size_locked(app: tauri::AppHandle, locked: bool) -> Result<()> {
    super::dispatch(async move {
        let _operation = super::operation().await?;
        let mut change = super::configuration::Change::new();
        let old_locked = change
            .before
            .clash_settings
            .window_size_locked
            .unwrap_or(false);
        let window = app
            .get_window("main")
            .context("main window is unavailable")?;
        let was_maximized = window.is_maximized()?;
        let result: Result<()> = async {
            if locked && was_maximized {
                window.unmaximize()?;
                for _ in 0..2 {
                    if !window.is_maximized()? {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                anyhow::ensure!(
                    !window.is_maximized()?,
                    "window has not finished restoring from maximized state"
                );
            }
            let (position, maximized) = crate::window::persist::read_window_state(&app)?;
            change.next.clash_settings.window_size_position =
                position.or(change.next.clash_settings.window_size_position);
            change.next.clash_settings.window_is_maximized = Some(maximized);
            change.next.clash_settings.window_size_locked = Some(locked);
            window.set_resizable(!locked)?;
            window.set_maximizable(!locked)?;
            super::configuration::apply_locked(change).await
        }
        .await;
        if let Err(error) = result {
            let mut recovery = Vec::new();
            if let Err(error) = window.set_resizable(!old_locked) {
                recovery.push(error.to_string());
            }
            if let Err(error) = window.set_maximizable(!old_locked) {
                recovery.push(error.to_string());
            }
            if was_maximized {
                if let Err(error) = window.maximize() {
                    recovery.push(error.to_string());
                }
            }
            if !recovery.is_empty() {
                anyhow::bail!(
                    "{error:#}; window recovery incomplete: {}",
                    recovery.join("; ")
                );
            }
            return Err(error);
        }
        Ok(())
    })
    .await
}
