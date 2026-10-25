use crate::config::*;
use crate::core::*;
use crate::log_err;
use crate::utils::resolve;
use serde_yaml::{Mapping, Value};
use tauri::Manager;

use super::verge_config::patch_verge;

// 打开面板
pub fn open_or_close_dashboard() {
    log::trace!("hotkey/dashboard entry received: open_or_close_dashboard");
    // 先复制出 AppHandle 再放锁：下面的窗口操作可能建窗，持着 Handle 锁时
    // 任何再取这把锁的调用（get_window/emit/app_resources_dir）都会自死锁。
    let Some(app_handle) = handle::Handle::global().app_handle.lock().clone() else {
        return;
    };
    if let Some(window) = app_handle.get_window("main") {
        if let Ok(true) = window.is_focused() {
            // 与关闭按钮同一路径：标记隐藏中，避免显示重试把窗口又弹回来
            resolve::mark_window_hiding_for(std::time::Duration::from_millis(1500));
            let _ = window.hide();
            return;
        }
    }
    log::trace!("hotkey/dashboard entry -> resolve::show_main_window");
    resolve::show_main_window(&app_handle);
}

// 重启clash
pub fn restart_clash_core() {
    tauri::async_runtime::spawn(async {
        match CoreManager::global().run_core().await {
            Ok(_) => {
                handle::Handle::refresh_clash();
                handle::Handle::notice_message("set_config::ok", "ok");
            }
            Err(err) => {
                handle::Handle::notice_message("set_config::error", format!("{err}"));
                log::error!(target:"app", "{err}");
            }
        }
    });
}

// 切换模式 rule/global/direct/script mode
pub fn change_clash_mode(mode: String) {
    let mut mapping = Mapping::new();
    mapping.insert(Value::from("mode"), mode.clone().into());

    tauri::async_runtime::spawn(async move {
        log::debug!(target: "app", "change clash mode to {mode}");

        match clash_api::patch_configs(&mapping).await {
            Ok(_) => {
                // 更新订阅
                Config::clash().data().patch_config(mapping);

                if Config::clash().data().save_config().is_ok() {
                    handle::Handle::refresh_clash();
                    log_err!(handle::Handle::update_systray_part());
                }
            }
            Err(err) => log::error!(target: "app", "{err}"),
        }
    });
}

// 切换系统代理
pub fn toggle_system_proxy() {
    // 用 data() 读取：draft() 有副作用，会创建草稿并读到其他流程未提交的中间值
    let enable = Config::verge().data().enable_system_proxy;
    let enable = enable.unwrap_or(false);

    tauri::async_runtime::spawn(async move {
        match patch_verge(IVerge {
            enable_system_proxy: Some(!enable),
            ..IVerge::default()
        })
        .await
        {
            Ok(_) => handle::Handle::refresh_verge(),
            Err(err) => {
                log::error!(target: "app", "{err}");
                handle::Handle::notice_message(
                    "set_config::error",
                    format!("Failed to toggle system proxy: {err:#}"),
                );
            }
        }
    });
}

// 切换tun模式
pub fn toggle_tun_mode() {
    let enable = Config::verge().data().enable_tun_mode;
    let enable = enable.unwrap_or(false);

    tauri::async_runtime::spawn(async move {
        match patch_verge(IVerge {
            enable_tun_mode: Some(!enable),
            ..IVerge::default()
        })
        .await
        {
            Ok(_) => handle::Handle::refresh_verge(),
            Err(err) => log::error!(target: "app", "{err}"),
        }
    });
}
