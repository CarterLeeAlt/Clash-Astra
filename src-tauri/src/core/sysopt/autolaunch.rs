use crate::config::Config;
use crate::log_err;
use anyhow::{anyhow, Result};
use auto_launch::AutoLaunchBuilder;
use tauri::utils::platform::current_exe;

use super::Sysopt;

impl Sysopt {
    /// init the auto launch
    pub fn init_launch(&self) -> Result<()> {
        let app_exe = current_exe()?;
        let app_exe = dunce::canonicalize(app_exe)?;
        let app_name = app_exe
            .file_stem()
            .and_then(|f| f.to_str())
            .ok_or(anyhow!("failed to get file stem"))?;

        let app_path = app_exe
            .as_os_str()
            .to_str()
            .ok_or(anyhow!("failed to get app_path"))?
            .to_string();

        // fix issue #26
        let app_path = format!("\"{app_path}\"");

        let auto = AutoLaunchBuilder::new()
            .set_app_name(app_name)
            .set_app_path(&app_path)
            .build()?;

        *self.auto_launch.lock() = Some(auto);

        Ok(())
    }

    /// update the startup
    pub fn update_launch(&self) -> Result<()> {
        let auto_launch = self.auto_launch.lock();

        if auto_launch.is_none() {
            drop(auto_launch);
            return self.init_launch();
        }
        let enable = { Config::verge().latest().enable_auto_launch };
        let enable = enable.unwrap_or(false);
        let auto_launch = auto_launch.as_ref().unwrap();

        match enable {
            true => auto_launch.enable()?,
            false => log_err!(auto_launch.disable()), // 忽略关闭的错误
        };

        Ok(())
    }
}
