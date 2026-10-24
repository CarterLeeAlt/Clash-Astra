use super::logging::init_log;
use super::{delete_log, init_panic_hook};
use crate::config::*;
use crate::utils::{dirs, help};
use anyhow::{Context, Result};
use std::fs;

/// Initialize all the config files
/// before tauri setup
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

    init_log()?;
    init_panic_hook();
    let _ = delete_log();

    let path = dirs::clash_path()?;
    if !path.exists() {
        help::save_yaml(&path, &IClashTemp::template().0, Some("# Clash Astra"))?;
    }

    let path = dirs::verge_path()?;
    if !path.exists() {
        help::save_yaml(&path, &IVerge::template(), Some("# Clash Astra"))?;
    }

    let path = dirs::profiles_path()?;
    if !path.exists() {
        help::save_yaml(&path, &IProfiles::template(), Some("# Clash Astra"))?;
    }

    Ok(())
}
