use crate::config::Config;
use crate::domain::window_lifecycle::{WindowLifecycle, WindowPhase};
use once_cell::sync::Lazy;
use parking_lot::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

static LIFECYCLE: Lazy<Mutex<WindowLifecycle>> =
    Lazy::new(|| Mutex::new(WindowLifecycle::default()));

/// 只在短小的状态转换中持锁；窗口 API 会切换线程，必须放锁后调用。
pub(crate) fn lifecycle() -> MutexGuard<'static, WindowLifecycle> {
    LIFECYCLE.lock()
}
pub(crate) fn lifecycle_snapshot() -> WindowLifecycle {
    lifecycle().clone()
}
pub(crate) fn latest_show_request_id() -> u64 {
    lifecycle().show_request
}
pub(crate) fn frontend_ready() -> bool {
    lifecycle().frontend_ready
}
pub(crate) fn set_window_alive() {
    lifecycle().phase = WindowPhase::Alive;
}
pub(crate) fn set_window_missing() {
    lifecycle().phase = WindowPhase::Missing;
}
pub(crate) fn pending_show_request() -> u64 {
    lifecycle().pending_show
}
pub(crate) fn take_pending_show_request() -> u64 {
    std::mem::take(&mut lifecycle().pending_show)
}
pub(crate) fn record_pending_show(request: u64) {
    let mut state = lifecycle();
    if state.desired_visible && !state.quitting {
        state.pending_show = state.pending_show.max(request);
    }
}

pub(crate) const WINDOW_SAVE_DEBOUNCE_MS: u64 = 600;
pub(crate) const WINDOW_SIZE_POSITION_SAVE_TASK: &str = "window.size_position.save";
pub(crate) const WINDOW_STATE_SAVE_TASK_PREFIX: &str = "window.state_save";
pub(crate) const WINDOW_SHOW_AFTER_HIDE_TASK: &str = "window.show.after_hide";
pub(crate) const WINDOW_SHOW_AFTER_HIDE_DELAY_MS: u64 = 300;
pub(crate) const WINDOW_SHOW_CREATE_FALLBACK_TASK: &str = "window.show.create_fallback";
pub(crate) const WINDOW_SHOW_CREATE_FALLBACK_DELAY_MS: u64 = 1500;
pub(crate) const WINDOW_SHOW_RETRIES_TASK: &str = "window.show.retries";
pub(crate) const WINDOW_SHOW_RETRIES_DELAY_MS: u64 = 200;
pub(crate) const HEARTBEAT_WARN_AFTER_MS: u64 = 15_000;

pub(crate) fn current_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowBuildMode {
    NormalConfigured,
    ForcedReliableFallback,
}
impl WindowBuildMode {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::NormalConfigured => "configured",
            Self::ForcedReliableFallback => "unexpected_destroyed",
        }
    }
}

pub(crate) fn configured_reliable_mode() -> bool {
    !Config::clash_settings()
        .enable_custom_frameless_window
        .unwrap_or(false)
}
pub(crate) fn effective_reliable_mode(mode: WindowBuildMode) -> bool {
    mode == WindowBuildMode::ForcedReliableFallback || configured_reliable_mode()
}
pub(crate) fn should_force_reliable_fallback(now: u64) -> bool {
    let state = lifecycle();
    state.phase == WindowPhase::UnexpectedDestroyed || state.destroys_in_window(now) >= 1
}
pub fn is_main_window_creating() -> bool {
    lifecycle().phase == WindowPhase::Creating
}
pub fn record_frontend_heartbeat() {
    lifecycle().heartbeat = current_time_millis();
}
pub fn record_frontend_error(message: String, stack: Option<String>) {
    log::error!(target: "app", "frontend error reported: message={}, stack={}", message, stack.unwrap_or_default());
}

pub fn check_main_window_health(app: &AppHandle, context: &str) {
    let Some(window) = app.get_window("main") else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let last = lifecycle().heartbeat;
    let elapsed = current_time_millis().saturating_sub(last);
    if last > 0 && elapsed > HEARTBEAT_WARN_AFTER_MS {
        log::warn!(target: "app", "webview may be hung: no frontend heartbeat for {elapsed}ms, context={context}");
    }
}

pub fn on_main_window_destroyed(app: &AppHandle) {
    let planned = super::webview_recovery::take_planned_recreate();
    let state = {
        let mut state = lifecycle();
        state.destroyed(current_time_millis(), planned.is_some());
        state.clone()
    };
    log::warn!(target: "app", "main window destroyed: phase={:?}, quitting={}, count={}", state.phase, state.quitting, state.destroy_count);
    if state.quitting {
        return;
    }
    if let Some(true) = planned {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            // Destroyed 回调返回后，Tauri 才移除旧的同名窗口。
            tokio::time::sleep(Duration::from_millis(500)).await;
            if !is_app_quitting() && lifecycle_snapshot().desired_visible {
                super::show::show_main_window(&app);
            }
        });
    }
}

pub(crate) fn main_window_recreate_allowed(now: u64) -> bool {
    lifecycle().can_create(now)
}
pub fn mark_window_hiding_for(duration: Duration) {
    lifecycle().request_hide(current_time_millis().saturating_add(duration.as_millis() as u64));
}
pub fn is_window_hiding_in_progress() -> bool {
    current_time_millis() < lifecycle().hide_until
}
pub fn set_app_quitting(value: bool) {
    let mut state = lifecycle();
    if value {
        state.quit();
    } else {
        state.quitting = false;
    }
}
pub fn is_app_quitting() -> bool {
    lifecycle().quitting
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ShowReason {
    Explicit,
    FrontendReady,
    CreateExisting,
    CreateFallback,
}

/// 代次防止旧窗口的延迟回调或析构释放新窗口的创建状态。
pub(crate) struct MainWindowCreatingGuard(u64);
impl MainWindowCreatingGuard {
    pub(crate) fn try_lock() -> Option<Self> {
        lifecycle().begin_create(current_time_millis()).map(Self)
    }
}
impl Drop for MainWindowCreatingGuard {
    fn drop(&mut self) {
        let mut state = lifecycle();
        if state.generation == self.0 && state.phase == WindowPhase::Creating {
            state.phase = WindowPhase::Missing;
        }
    }
}

pub(crate) struct MainWindowShowingGuard(u64);
impl MainWindowShowingGuard {
    pub(crate) fn try_lock(request: u64, _reason: ShowReason) -> Option<Self> {
        let mut state = lifecycle();
        if !state.desired_visible || state.quitting || state.showing.is_some() {
            return None;
        }
        state.showing = Some(request);
        Some(Self(request))
    }
}
impl Drop for MainWindowShowingGuard {
    fn drop(&mut self) {
        let mut state = lifecycle();
        if state.showing == Some(self.0) {
            state.showing = None;
        }
    }
}
