use crate::config::Config;
use anyhow::{anyhow, Result};
use auto_launch::AutoLaunchBuilder;
use tauri::utils::platform::current_exe;

use super::Sysopt;

impl Sysopt {
    /// 根据当前可执行文件创建自动启动项适配器。
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

        // 可执行文件路径加引号，保证包含空格时仍被作为完整路径解析。
        let app_path = format!("\"{app_path}\"");

        let auto = AutoLaunchBuilder::new()
            .set_app_name(app_name)
            .set_app_path(&app_path)
            .build()?;

        *self.auto_launch.lock() = Some(auto);

        Ok(())
    }

    /// 从已提交设置同步自动启动项。
    pub fn update_launch(&self) -> Result<()> {
        self.apply_launch(Config::verge().enable_auto_launch.unwrap_or(false))
    }

    pub fn apply_launch(&self, enable: bool) -> Result<()> {
        if self.auto_launch.lock().is_none() {
            self.init_launch()?;
        }
        let auto_launch = self.auto_launch.lock();

        if auto_launch.is_none() {
            drop(auto_launch);
            return self.init_launch();
        }
        let auto_launch = auto_launch.as_ref().unwrap();

        match enable {
            true => auto_launch.enable()?,
            false => auto_launch.disable()?,
        };

        Ok(())
    }
}
