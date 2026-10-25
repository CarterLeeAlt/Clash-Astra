use super::tray::Tray;
use crate::log_err;
use crate::logging::app_event::AppLogEvent;
use anyhow::{bail, Result};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{AppHandle, Manager, Window};

#[derive(Debug, Default, Clone)]
pub struct Handle {
    pub app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl Handle {
    pub fn global() -> &'static Handle {
        static HANDLE: OnceCell<Handle> = OnceCell::new();

        HANDLE.get_or_init(|| Handle {
            app_handle: Arc::new(Mutex::new(None)),
        })
    }

    pub fn init(&self, app_handle: AppHandle) {
        *self.app_handle.lock() = Some(app_handle);
    }

    pub fn get_window(&self) -> Option<Window> {
        self.app_handle
            .lock()
            .as_ref()
            .and_then(|a| a.get_window("main"))
    }

    pub fn refresh_clash() {
        if let Some(window) = Self::global().get_window() {
            log_err!(window.emit("verge://refresh-clash-config", "yes"));
        }
    }

    pub fn refresh_verge() {
        if let Some(window) = Self::global().get_window() {
            log_err!(window.emit("verge://refresh-verge-config", "yes"));
        }
    }

    pub fn notice_message<S: Into<String>, M: Into<String>>(status: S, msg: M) {
        if let Some(window) = Self::global().get_window() {
            log_err!(window.emit("verge://notice-message", (status.into(), msg.into())));
        }
    }

    /// 一轮后台自动延迟测试完成后通知前端刷新；manual 与自动走同一套延迟缓存。
    pub fn notify_auto_delay_finished() {
        if let Some(window) = Self::global().get_window() {
            log_err!(window.emit("verge://auto-delay-finished", "yes"));
        }
    }

    /// 一轮站点检测完成后推送结果（URL -> 延迟/出口代理）。
    pub fn notify_auto_site_test_results(
        results: &std::collections::HashMap<String, crate::feat::TestDelayResult>,
    ) {
        if let Some(window) = Self::global().get_window() {
            log_err!(window.emit("verge://auto-site-test-results", results));
        }
    }

    /// 单个站点完成时推送，前端逐张卡片更新。
    pub fn notify_auto_site_test_result(event: &crate::feat::SiteTestResultEvent) {
        if let Some(window) = Self::global().get_window() {
            log_err!(window.emit("verge://auto-site-test-result", event));
        }
    }

    pub fn emit_log<T: Into<String>, M: Into<String>>(level: T, msg: M) {
        if let Some(window) = Self::global().get_window() {
            let event = AppLogEvent {
                r#type: level.into(),
                payload: msg.into(),
            };
            log_err!(window.emit("verge://app-log", event));
        }
    }

    pub fn update_systray() -> Result<()> {
        let app_handle = Self::global().app_handle.lock();
        if app_handle.is_none() {
            bail!("update_systray unhandled error");
        }
        Tray::update_systray(app_handle.as_ref().unwrap())?;
        Ok(())
    }

    /// update the system tray state
    pub fn update_systray_part() -> Result<()> {
        let app_handle = Self::global().app_handle.lock();
        if app_handle.is_none() {
            bail!("update_systray unhandled error");
        }
        Tray::update_part(app_handle.as_ref().unwrap())?;
        Ok(())
    }
}
