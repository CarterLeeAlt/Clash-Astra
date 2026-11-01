use super::logging::init_log;
use super::{delete_log, init_panic_hook};
use crate::config::*;
use crate::utils::{dirs, help};
use anyhow::{Context, Result};
use std::fs;

/// Tauri setup 前恢复未完成事务，并准备缺失的默认配置文件。
pub fn init_config() -> Result<()> {
    let app_dir = dirs::app_home_dir()?;
    fs::create_dir_all(&app_dir).with_context(|| {
        format!(
            "failed to create portable application directory {}",
            app_dir.display()
        )
    })?;
    let profiles_dir = dirs::app_profiles_dir()?;
    fs::create_dir_all(&profiles_dir).with_context(|| {
        format!(
            "failed to create portable profiles directory {}",
            profiles_dir.display()
        )
    })?;

    crate::infrastructure::config_repository::FileTransaction::recover(app_dir)?;
    init_log()?;
    init_panic_hook();
    let _ = delete_log();

    let path = dirs::clash_path()?;
    if !path.exists() {
        help::save_yaml(&path, &IClashTemp::template().0, Some("# Clash Astra"))?;
    }

    let path = dirs::clash_settings_path()?;
    if !path.exists() {
        help::save_yaml(&path, &IClashSettings::template(), Some("# Clash Astra"))?;
    }

    let path = dirs::profiles_path()?;
    if !path.exists() {
        help::save_yaml(&path, &IProfiles::template(), Some("# Clash Astra"))?;
    }

    Ok(())
}
