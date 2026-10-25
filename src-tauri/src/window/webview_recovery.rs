//! WebView2 进程故障的检测与恢复。
//!
//! 渲染进程崩溃（内存耗尽等）后窗口只剩白屏，页面里的 JS 已经不存在，
//! 前端无法自救，只能由后端在原生层收到 ProcessFailed 后处理：
//! - 渲染进程退出：原生 Reload 重建渲染进程；
//! - 渲染进程持续无响应：等一段时间仍无响应再 Reload；
//! - 浏览器进程退出：整个 WebView 已失效，关闭并重建主窗口；
//! - GPU/工具进程退出：WebView2 会自行重启，只记日志。
//!
//! 自动恢复都有次数上限，避免崩溃循环时无限重载或重建。

use super::state::{current_time_millis, is_app_quitting};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Window};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2Controller, COREWEBVIEW2_PROCESS_FAILED_KIND,
    COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
};
use webview2_com::ProcessFailedEventHandler;
use windows::Win32::System::WinRT::EventRegistrationToken;

const RECOVERY_WINDOW_MS: u64 = 5 * 60 * 1000;
const MAX_RELOADS_PER_WINDOW: usize = 3;
const MAX_RECREATES_PER_WINDOW: usize = 2;
/// 无响应事件持续这么久才重载：页面短时繁忙（大表格首屏）不应被重载。
const UNRESPONSIVE_RELOAD_AFTER_MS: u64 = 20_000;
/// 两次无响应事件间隔超过这个时间，视为渲染进程已恢复、下一次是新的一轮无响应。
/// 必须明显短于重载阈值，否则上一轮的起点会让新一轮的第一个事件就触发重载。
const UNRESPONSIVE_STREAK_GAP_MS: u64 = 15_000;

static RELOAD_HISTORY: Mutex<Vec<u64>> = Mutex::new(Vec::new());
static RECREATE_HISTORY: Mutex<Vec<u64>> = Mutex::new(Vec::new());
static UNRESPONSIVE_SINCE_MS: AtomicU64 = AtomicU64::new(0);
static UNRESPONSIVE_LAST_MS: AtomicU64 = AtomicU64::new(0);

/// 主窗口正在为恢复而计划性重建：关闭不拦截、销毁不计入意外销毁。
static PLANNED_RECREATE: AtomicBool = AtomicBool::new(false);
static RECREATE_SHOW: AtomicBool = AtomicBool::new(false);

pub fn is_planned_recreate() -> bool {
    PLANNED_RECREATE.load(Ordering::SeqCst)
}

/// 主窗口销毁时调用；若是计划性重建则清除标记并返回重建后是否需要显示。
pub fn take_planned_recreate() -> Option<bool> {
    if PLANNED_RECREATE.swap(false, Ordering::SeqCst) {
        Some(RECREATE_SHOW.swap(false, Ordering::SeqCst))
    } else {
        None
    }
}

/// 建窗后注册 ProcessFailed 监听。注册失败只记日志，不影响窗口使用。
pub fn install(window: &Window) {
    let app_handle = window.app_handle();
    let result = window.with_webview(move |webview| {
        if let Err(err) = register(webview.controller(), app_handle) {
            log::warn!(target: "app", "failed to register WebView2 ProcessFailed handler: {err}");
        }
    });
    if let Err(err) = result {
        log::warn!(target: "app", "failed to access WebView2 for ProcessFailed handler: {err}");
    }
}

/// 原生重载页面。与 `eval("location.reload()")` 不同，JS 卡死或渲染进程已崩溃时也有效。
pub fn reload(window: &Window) -> tauri::Result<()> {
    window.with_webview(|webview| unsafe {
        match webview.controller().CoreWebView2() {
            Ok(core) => {
                if let Err(err) = core.Reload() {
                    log::warn!(target: "app", "WebView2 native reload failed: {err}");
                }
            }
            Err(err) => log::warn!(target: "app", "WebView2 core unavailable for reload: {err}"),
        }
    })
}

fn register(
    controller: ICoreWebView2Controller,
    app_handle: AppHandle,
) -> windows::core::Result<()> {
    unsafe {
        let core = controller.CoreWebView2()?;
        let mut token = EventRegistrationToken::default();
        core.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |sender, args| {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(-1);
                if let Some(args) = args {
                    args.ProcessFailedKind(&mut kind)?;
                }
                on_process_failed(&app_handle, sender, kind);
                Ok(())
            })),
            &mut token,
        )?;
    }
    log::info!(target: "app", "WebView2 ProcessFailed handler registered");
    Ok(())
}

fn on_process_failed(
    app_handle: &AppHandle,
    sender: Option<ICoreWebView2>,
    kind: COREWEBVIEW2_PROCESS_FAILED_KIND,
) {
    if is_app_quitting() {
        return;
    }
    let now = current_time_millis();

    if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED {
        log::error!(target: "app", "WebView2 render process exited (blank page); reloading");
        UNRESPONSIVE_SINCE_MS.store(0, Ordering::SeqCst);
        reload_sender(sender, now);
    } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE {
        let last = UNRESPONSIVE_LAST_MS.swap(now, Ordering::SeqCst);
        let since = UNRESPONSIVE_SINCE_MS.load(Ordering::SeqCst);
        if since == 0 || now.saturating_sub(last) > UNRESPONSIVE_STREAK_GAP_MS {
            UNRESPONSIVE_SINCE_MS.store(now, Ordering::SeqCst);
            log::warn!(target: "app", "WebView2 render process unresponsive; waiting before reload");
            return;
        }
        let unresponsive_for = now.saturating_sub(since);
        if unresponsive_for < UNRESPONSIVE_RELOAD_AFTER_MS {
            return;
        }
        log::error!(
            target: "app",
            "WebView2 render process unresponsive for {unresponsive_for}ms; reloading"
        );
        UNRESPONSIVE_SINCE_MS.store(0, Ordering::SeqCst);
        reload_sender(sender, now);
    } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
        log::error!(target: "app", "WebView2 browser process exited; recreating main window");
        recreate_main_window(app_handle, now);
    } else {
        log::warn!(
            target: "app",
            "WebView2 auxiliary process failed, kind={}; WebView2 recovers it automatically",
            kind.0
        );
    }
}

fn reload_sender(sender: Option<ICoreWebView2>, now: u64) {
    if !claim(&RELOAD_HISTORY, now, MAX_RELOADS_PER_WINDOW) {
        log::error!(
            target: "app",
            "WebView2 auto reload limit reached ({MAX_RELOADS_PER_WINDOW} in {RECOVERY_WINDOW_MS}ms); use tray Reload UI"
        );
        return;
    }
    if let Some(core) = sender {
        if let Err(err) = unsafe { core.Reload() } {
            log::error!(target: "app", "WebView2 reload after process failure failed: {err}");
        }
    }
}

fn recreate_main_window(app_handle: &AppHandle, now: u64) {
    if !claim(&RECREATE_HISTORY, now, MAX_RECREATES_PER_WINDOW) {
        log::error!(
            target: "app",
            "main window recreate limit reached ({MAX_RECREATES_PER_WINDOW} in {RECOVERY_WINDOW_MS}ms); restart app from tray"
        );
        return;
    }
    // 窗口 API 一律放到 COM 回调之外的线程执行：回调运行在主线程消息泵里，
    // 在这里同步调用窗口 API 可能重入运行时的窗口表。
    let app_handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let Some(window) = app_handle.get_window("main") else {
            return;
        };
        let visible = window.is_visible().unwrap_or(true);
        RECREATE_SHOW.store(visible, Ordering::SeqCst);
        PLANNED_RECREATE.store(true, Ordering::SeqCst);
        // Destroyed 事件里按 take_planned_recreate 重建
        if let Err(err) = window.close() {
            PLANNED_RECREATE.store(false, Ordering::SeqCst);
            log::error!(target: "app", "failed to close dead main window for recreate: {err}");
        }
    });
}

/// 在 `RECOVERY_WINDOW_MS` 窗口内记录一次恢复，次数已满时返回 false。
fn claim(history: &Mutex<Vec<u64>>, now: u64, limit: usize) -> bool {
    let mut history = history.lock().unwrap_or_else(|err| err.into_inner());
    history.retain(|time| now.saturating_sub(*time) < RECOVERY_WINDOW_MS);
    if history.len() >= limit {
        return false;
    }
    history.push(now);
    true
}
