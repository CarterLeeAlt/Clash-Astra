use crate::domain::hotkeys::{self, HotkeyMap};
use crate::{application::actions as feat, config::Config};
use anyhow::{bail, Result};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{AppHandle, GlobalShortcutManager};

pub struct Hotkey {
    current: Arc<Mutex<HotkeyMap>>, // 记录实际注册成功的快捷键，便于部分失败后补偿。

    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl Hotkey {
    pub fn global() -> &'static Hotkey {
        static HOTKEY: OnceCell<Hotkey> = OnceCell::new();

        HOTKEY.get_or_init(|| Hotkey {
            current: Arc::new(Mutex::new(HotkeyMap::new())),
            app_handle: Arc::new(Mutex::new(None)),
        })
    }

    pub fn init(&self, app_handle: AppHandle) -> Result<()> {
        *self.app_handle.lock() = Some(app_handle);

        self.update(Config::verge().hotkeys.unwrap_or_default())
    }

    fn get_manager(&self) -> Result<impl GlobalShortcutManager> {
        let app_handle = self.app_handle.lock().clone();
        if app_handle.is_none() {
            bail!("failed to get the hotkey manager");
        }
        Ok(app_handle.as_ref().unwrap().global_shortcut_manager())
    }

    fn register(&self, hotkey: &str, func: &str) -> Result<()> {
        let mut manager = self.get_manager()?;

        let f = match func.trim() {
            "open_or_close_dashboard" => feat::open_or_close_dashboard,
            "clash_mode_rule" => || feat::change_clash_mode("rule".into()),
            "clash_mode_global" => || feat::change_clash_mode("global".into()),
            "clash_mode_direct" => || feat::change_clash_mode("direct".into()),
            "toggle_system_proxy" => feat::toggle_system_proxy,
            "toggle_tun_mode" => feat::toggle_tun_mode,

            _ => bail!("invalid function \"{func}\""),
        };

        manager.register(hotkey, f)?;
        log::info!(target: "app", "register hotkey {hotkey} {func}");
        Ok(())
    }

    fn unregister(&self, hotkey: &str) -> Result<()> {
        self.get_manager()?.unregister(hotkey)?;
        log::info!(target: "app", "unregister hotkey {hotkey}");
        Ok(())
    }

    pub fn update(&self, new_hotkeys: Vec<String>) -> Result<()> {
        let desired = hotkeys::parse(&new_hotkeys)?;
        let mut current = self.current.lock();
        let removed: Vec<_> = current
            .iter()
            .filter(|(key, action)| desired.get(*key) != Some(*action))
            .map(|(key, _)| key.clone())
            .collect();
        for key in removed {
            self.unregister(&key)?;
            current.remove(&key);
        }
        for (key, action) in desired {
            if current.get(&key) == Some(&action) {
                continue;
            }
            self.register(&key, &action)?;
            current.insert(key, action);
        }
        Ok(())
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        if let Ok(mut manager) = self.get_manager() {
            let _ = manager.unregister_all();
        }
    }
}
