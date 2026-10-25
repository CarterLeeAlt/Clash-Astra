use super::persist::schedule_window_state_save;
use crate::config::Config;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

pub(crate) static MAIN_WINDOW_CREATING: AtomicBool = AtomicBool::new(false);
pub(crate) static FRONTEND_READY_LISTENING: AtomicBool = AtomicBool::new(false);
pub(crate) static FRONTEND_READY_HANDLED: AtomicBool = AtomicBool::new(false);
pub(crate) static FRONTEND_SHOW_WHEN_READY: AtomicBool = AtomicBool::new(true);
pub(crate) static MAIN_WINDOW_SHOW_REQUEST_ID: AtomicU64 = AtomicU64::new(0);
pub(crate) static MAIN_WINDOW_SHOWING: AtomicBool = AtomicBool::new(false);
pub(crate) static MAIN_WINDOW_SHOW_PENDING_ID: AtomicU64 = AtomicU64::new(0);
pub(crate) static IS_APP_QUITTING: AtomicBool = AtomicBool::new(false);

pub(crate) const MAIN_WINDOW_STATE_MISSING: u64 = 0;
pub(crate) const MAIN_WINDOW_STATE_CREATING: u64 = 1;
pub(crate) const MAIN_WINDOW_STATE_ALIVE: u64 = 2;
pub(crate) const MAIN_WINDOW_STATE_UNEXPECTED_DESTROYED: u64 = 3;
pub(crate) const MAIN_WINDOW_STATE_RECREATE_BACKOFF: u64 = 4;
pub(crate) const MAIN_WINDOW_RECREATE_BACKOFF_MS: u64 = 3_000;
pub(crate) const MAIN_WINDOW_DESTROY_WINDOW_MS: u64 = 60_000;
pub(crate) const MAIN_WINDOW_MAX_RECREATE_DESTROYS: u64 = 3;
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

pub(crate) static MAIN_WINDOW_STATE: AtomicU64 = AtomicU64::new(MAIN_WINDOW_STATE_MISSING);
pub(crate) static MAIN_WINDOW_UNEXPECTED_DESTROY_COUNT: AtomicU64 = AtomicU64::new(0);
pub(crate) static MAIN_WINDOW_UNEXPECTED_DESTROY_WINDOW_START_MS: AtomicU64 = AtomicU64::new(0);
pub(crate) static MAIN_WINDOW_LAST_UNEXPECTED_DESTROY_MS: AtomicU64 = AtomicU64::new(0);
pub(crate) static MAIN_WINDOW_LAST_CREATE_ATTEMPT_MS: AtomicU64 = AtomicU64::new(0);
pub(crate) static WINDOW_HIDE_IN_PROGRESS_UNTIL_MS: AtomicU64 = AtomicU64::new(0);
pub(crate) static LAST_FRONTEND_HEARTBEAT_MS: AtomicU64 = AtomicU64::new(0);

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

#[cfg(target_os = "windows")]
pub(crate) fn configured_reliable_mode() -> bool {
    !Config::verge()
        .latest()
        .enable_custom_frameless_window
        .unwrap_or(false)
}

pub(crate) fn effective_reliable_mode(build_mode: WindowBuildMode) -> bool {
    match build_mode {
        WindowBuildMode::ForcedReliableFallback => true,
        WindowBuildMode::NormalConfigured => configured_reliable_mode(),
    }
}

pub(crate) fn main_window_state_name(state: u64) -> &'static str {
    match state {
        MAIN_WINDOW_STATE_MISSING => "Missing",
        MAIN_WINDOW_STATE_CREATING => "Creating",
        MAIN_WINDOW_STATE_ALIVE => "Alive",
        MAIN_WINDOW_STATE_UNEXPECTED_DESTROYED => "UnexpectedDestroyed",
        MAIN_WINDOW_STATE_RECREATE_BACKOFF => "RecreateBackoff",
        _ => "Unknown",
    }
}

pub(crate) fn unexpected_destroy_count_in_window(now: u64) -> u64 {
    let start = MAIN_WINDOW_UNEXPECTED_DESTROY_WINDOW_START_MS.load(Ordering::SeqCst);
    let count = MAIN_WINDOW_UNEXPECTED_DESTROY_COUNT.load(Ordering::SeqCst);
    if start > 0 && now.saturating_sub(start) <= MAIN_WINDOW_DESTROY_WINDOW_MS {
        count
    } else {
        0
    }
}

pub(crate) fn should_force_reliable_fallback(now: u64) -> bool {
    MAIN_WINDOW_STATE.load(Ordering::SeqCst) == MAIN_WINDOW_STATE_UNEXPECTED_DESTROYED
        || unexpected_destroy_count_in_window(now) >= 1
}

pub fn is_main_window_creating() -> bool {
    MAIN_WINDOW_CREATING.load(Ordering::SeqCst)
}

pub fn record_frontend_heartbeat() {
    LAST_FRONTEND_HEARTBEAT_MS.store(current_time_millis(), Ordering::SeqCst);
}

pub fn record_frontend_error(message: String, stack: Option<String>) {
    log::error!(
        target: "app",
        "frontend error reported: message={}, stack={}",
        message,
        stack.unwrap_or_default()
    );
}

pub fn check_main_window_health(app_handle: &AppHandle, context: &str) {
    let Some(window) = app_handle.get_window("main") else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let last = LAST_FRONTEND_HEARTBEAT_MS.load(Ordering::SeqCst);
    let now = current_time_millis();
    if last == 0 || now.saturating_sub(last) <= HEARTBEAT_WARN_AFTER_MS {
        return;
    }

    log::warn!(
        target: "app",
        "webview may be hung: no frontend heartbeat for {}ms, context={}",
        now.saturating_sub(last),
        context
    );
}

pub fn on_main_window_destroyed(app_handle: &AppHandle) {
    let is_quitting = is_app_quitting();
    let now = current_time_millis();
    log::warn!(
        target: "app",
        "main window destroyed, is_quitting={}",
        is_quitting
    );

    if is_quitting {
        MAIN_WINDOW_STATE.store(MAIN_WINDOW_STATE_MISSING, Ordering::SeqCst);
        schedule_window_state_save(app_handle.clone(), Duration::from_millis(250));
        return;
    }

    // WebView2 浏览器进程退出后的计划性重建：不计入意外销毁，直接重建。
    #[cfg(target_os = "windows")]
    if let Some(show) = super::webview_recovery::take_planned_recreate() {
        MAIN_WINDOW_STATE.store(MAIN_WINDOW_STATE_MISSING, Ordering::SeqCst);
        MAIN_WINDOW_CREATING.store(false, Ordering::SeqCst);
        MAIN_WINDOW_SHOWING.store(false, Ordering::SeqCst);
        MAIN_WINDOW_SHOW_PENDING_ID.store(0, Ordering::SeqCst);
        FRONTEND_READY_HANDLED.store(false, Ordering::SeqCst);
        log::warn!(target: "app", "main window destroyed for planned recreate, show={show}");
        // 原本隐藏的窗口不立即重建（重建流程会在前端就绪后把窗口显示出来），
        // 保持 Missing，用户下次打开时由 show_main_window 新建。
        if show {
            // 稍后重建：Destroyed 回调里旧窗口可能还没从管理器移除，同名 "main" 会建窗失败
            let app_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                super::show::show_main_window(&app_handle);
            });
        }
        return;
    }

    let window_start = MAIN_WINDOW_UNEXPECTED_DESTROY_WINDOW_START_MS.load(Ordering::SeqCst);
    if window_start == 0 || now.saturating_sub(window_start) > MAIN_WINDOW_DESTROY_WINDOW_MS {
        MAIN_WINDOW_UNEXPECTED_DESTROY_WINDOW_START_MS.store(now, Ordering::SeqCst);
        MAIN_WINDOW_UNEXPECTED_DESTROY_COUNT.store(1, Ordering::SeqCst);
    } else {
        MAIN_WINDOW_UNEXPECTED_DESTROY_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    let destroy_count = MAIN_WINDOW_UNEXPECTED_DESTROY_COUNT.load(Ordering::SeqCst);
    MAIN_WINDOW_LAST_UNEXPECTED_DESTROY_MS.store(now, Ordering::SeqCst);
    MAIN_WINDOW_STATE.store(MAIN_WINDOW_STATE_UNEXPECTED_DESTROYED, Ordering::SeqCst);
    MAIN_WINDOW_CREATING.store(false, Ordering::SeqCst);
    MAIN_WINDOW_SHOWING.store(false, Ordering::SeqCst);
    MAIN_WINDOW_SHOW_PENDING_ID.store(0, Ordering::SeqCst);
    FRONTEND_READY_HANDLED.store(false, Ordering::SeqCst);

    log::error!(
        target: "app",
        "main window unexpected destroyed: unexpected_destroyed_count={}, last_show_request_id={}, reliable_mode={}, state={}, action=wait_for_user_backoff",
        destroy_count,
        MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst),
        configured_reliable_mode(),
        main_window_state_name(MAIN_WINDOW_STATE.load(Ordering::SeqCst))
    );

    if destroy_count >= 2 {
        log::warn!(
            target: "app",
            "main window unexpected destroyed {} times within 60s; custom frameless mode will be disabled for fallback rebuilds",
            destroy_count
        );
    }
    if destroy_count >= MAIN_WINDOW_MAX_RECREATE_DESTROYS {
        MAIN_WINDOW_STATE.store(MAIN_WINDOW_STATE_RECREATE_BACKOFF, Ordering::SeqCst);
        log::error!(
            target: "app",
            "main window unexpected destroyed {} times within 60s; automatic rebuild stopped, please restart app or open logs directory",
            destroy_count
        );
    }
}

pub(crate) fn main_window_recreate_allowed(now: u64) -> bool {
    if MAIN_WINDOW_CREATING.load(Ordering::SeqCst) {
        log::warn!(target: "app", "main window show ignored because creation is already in progress; pending_show recorded");
        return false;
    }

    let mut state = MAIN_WINDOW_STATE.load(Ordering::SeqCst);
    // 退避只在 60s 计数窗口内有效；窗口期过后必须允许重建，
    // 否则状态一直停在 RecreateBackoff，托盘、热键、单例唤起永远打不开窗口。
    if state == MAIN_WINDOW_STATE_RECREATE_BACKOFF
        && unexpected_destroy_count_in_window(now) < MAIN_WINDOW_MAX_RECREATE_DESTROYS
    {
        log::warn!(target: "app", "main window recreate backoff expired; allow rebuild");
        MAIN_WINDOW_STATE.store(MAIN_WINDOW_STATE_MISSING, Ordering::SeqCst);
        state = MAIN_WINDOW_STATE_MISSING;
    }
    if unexpected_destroy_count_in_window(now) >= MAIN_WINDOW_MAX_RECREATE_DESTROYS {
        log::error!(
            target: "app",
            "main window recreate blocked by backoff: state={}, unexpected_destroyed_count={}, last_show_request_id={}",
            main_window_state_name(state),
            unexpected_destroy_count_in_window(now),
            MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst)
        );
        return false;
    }

    let last_create = MAIN_WINDOW_LAST_CREATE_ATTEMPT_MS.load(Ordering::SeqCst);
    let last_destroy = MAIN_WINDOW_LAST_UNEXPECTED_DESTROY_MS.load(Ordering::SeqCst);
    if last_destroy > 0
        && last_create > 0
        && now.saturating_sub(last_create) < MAIN_WINDOW_RECREATE_BACKOFF_MS
    {
        log::warn!(
            target: "app",
            "main window recreate ignored by backoff, elapsed_since_create_ms={}, backoff_ms={}, last_destroy_ms={}",
            now.saturating_sub(last_create),
            MAIN_WINDOW_RECREATE_BACKOFF_MS,
            last_destroy
        );
        return false;
    }

    true
}

pub fn mark_window_hiding_for(duration: Duration) {
    let until = current_time_millis().saturating_add(duration.as_millis() as u64);
    WINDOW_HIDE_IN_PROGRESS_UNTIL_MS.store(until, Ordering::SeqCst);
}

pub fn is_window_hiding_in_progress() -> bool {
    let until = WINDOW_HIDE_IN_PROGRESS_UNTIL_MS.load(Ordering::SeqCst);
    until > 0 && current_time_millis() < until
}

pub fn set_app_quitting(value: bool) {
    IS_APP_QUITTING.store(value, Ordering::SeqCst);
}

pub fn is_app_quitting() -> bool {
    IS_APP_QUITTING.load(Ordering::SeqCst)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ShowReason {
    Explicit,
    FrontendReady,
    FrontendReadyRestore,
    CreateExisting,
    CreateFallback,
}

pub(crate) struct MainWindowCreatingGuard;

impl MainWindowCreatingGuard {
    pub(crate) fn try_lock() -> Option<Self> {
        if MAIN_WINDOW_CREATING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            log::trace!("create_window skipped because main window is already creating");
            None
        } else {
            MAIN_WINDOW_STATE.store(MAIN_WINDOW_STATE_CREATING, Ordering::SeqCst);
            Some(Self)
        }
    }
}

impl Drop for MainWindowCreatingGuard {
    fn drop(&mut self) {
        MAIN_WINDOW_CREATING.store(false, Ordering::SeqCst);
        log::trace!("create_window creating lock released");
    }
}

pub(crate) struct MainWindowShowingGuard;

impl MainWindowShowingGuard {
    pub(crate) fn try_lock(request_id: u64, reason: ShowReason) -> Option<Self> {
        if MAIN_WINDOW_SHOWING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            log::trace!(
                "show skipped/merged because show already in progress, request_id={}, reason={:?}",
                request_id,
                reason
            );
            None
        } else {
            Some(Self)
        }
    }
}

impl Drop for MainWindowShowingGuard {
    fn drop(&mut self) {
        MAIN_WINDOW_SHOWING.store(false, Ordering::SeqCst);
        log::trace!("main window show lock released");
    }
}
