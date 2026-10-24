use super::create::create_window;
use super::state::*;
use crate::core::scheduler::{CancellableJobFn, JobFn, JobFuture, OneShotScheduler};
use crate::trace_err;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub(crate) fn register_frontend_ready_listener(app_handle: &AppHandle, show_when_ready: bool) {
    FRONTEND_SHOW_WHEN_READY.store(show_when_ready, Ordering::SeqCst);

    if FRONTEND_READY_LISTENING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        log::trace!(
            "frontend ready listener already registered, show_when_ready={}",
            show_when_ready
        );
        return;
    }

    let listener_app_handle = app_handle.clone();
    let ready_app_handle = app_handle.clone();
    log::trace!(
        "register frontend ready listener, show_when_ready={}",
        show_when_ready
    );
    listener_app_handle.listen_global("frontend://ready", move |_| {
        if FRONTEND_READY_HANDLED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            log::trace!("frontend ready already handled, skip");
            return;
        }

        let show_when_ready = FRONTEND_SHOW_WHEN_READY.load(Ordering::SeqCst);
        let show_request_id = MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst);

        #[cfg(target_os = "windows")]
        crate::core::win_firewall::notify_lan_firewall_if_needed();
        log::trace!(
            "frontend ready received, show_when_ready={}, show_request_id={}",
            show_when_ready,
            show_request_id
        );

        if show_when_ready || show_request_id > 0 {
            log::trace!(
                "frontend ready wants show, show_when_ready={}, show_request_id={}",
                show_when_ready,
                show_request_id
            );
            if let Some(window) = ready_app_handle.get_window("main") {
                show_existing_window(&window, show_request_id, ShowReason::FrontendReady);
            } else {
                log::trace!("frontend ready wants show but main window is missing");
                create_window(&ready_app_handle, true);
            }
            return;
        }

        if let Some(window) = ready_app_handle.get_window("main") {
            log::trace!(
                "frontend ready wants hide, show_request_id={}",
                show_request_id
            );
            let latest_show_request_id = MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst);
            if latest_show_request_id > 0 {
                log::trace!(
                    "frontend ready hide skipped because show requested, show_request_id={}",
                    latest_show_request_id
                );
                show_existing_window(
                    &window,
                    latest_show_request_id,
                    ShowReason::FrontendReadyRestore,
                );
                return;
            }

            trace_err!(window.hide(), "set win hidden after frontend ready");
            let latest_show_request_id = MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst);
            if latest_show_request_id > 0 {
                log::trace!(
                    "frontend ready post-hide show requested, restore main window, show_request_id={}",
                    latest_show_request_id
                );
                show_existing_window(
                    &window,
                    latest_show_request_id,
                    ShowReason::FrontendReadyRestore,
                );
            } else {
                log::trace!("frontend ready hide completed without show request");
            }
        } else {
            log::trace!("frontend ready wants hide but main window is missing while silent");
        }
    });
}

fn log_window_state(window: &tauri::Window, context: &str) {
    let visible = window
        .is_visible()
        .map(|value| value.to_string())
        .unwrap_or_else(|err| format!("err:{err}"));
    let minimized = window
        .is_minimized()
        .map(|value| value.to_string())
        .unwrap_or_else(|err| format!("err:{err}"));
    let focused = window
        .is_focused()
        .map(|value| value.to_string())
        .unwrap_or_else(|err| format!("err:{err}"));
    log::trace!(
        "{} window state: visible={}, minimized={}, focused={}",
        context,
        visible,
        minimized,
        focused
    );
}

fn ensure_main_window_onscreen(window: &tauri::Window, context: &str) {
    let position = match window.outer_position() {
        Ok(position) => position,
        Err(err) => {
            log::trace!(
                "main window offscreen check skipped, context={}, position error={}",
                context,
                err
            );
            return;
        }
    };
    let size = match window.outer_size() {
        Ok(size) => size,
        Err(err) => {
            log::trace!(
                "main window offscreen check skipped, context={}, size error={}",
                context,
                err
            );
            return;
        }
    };
    let monitors = match window.available_monitors() {
        Ok(monitors) => monitors,
        Err(err) => {
            log::trace!(
                "main window offscreen check skipped, context={}, monitors error={}",
                context,
                err
            );
            return;
        }
    };

    if monitors.is_empty() {
        log::trace!(
            "main window offscreen check skipped, context={}, no monitors available",
            context
        );
        return;
    }

    let window_left = position.x as i64;
    let window_top = position.y as i64;
    let window_right = window_left + size.width as i64;
    let window_bottom = window_top + size.height as i64;

    let intersects_monitor = monitors.iter().any(|monitor| {
        let monitor_position = monitor.position();
        let monitor_size = monitor.size();
        let monitor_left = monitor_position.x as i64;
        let monitor_top = monitor_position.y as i64;
        let monitor_right = monitor_left + monitor_size.width as i64;
        let monitor_bottom = monitor_top + monitor_size.height as i64;

        window_left < monitor_right
            && window_right > monitor_left
            && window_top < monitor_bottom
            && window_bottom > monitor_top
    });

    if !intersects_monitor {
        log::trace!(
            "main window appears offscreen, recentering, context={}, window=({}, {})-({}, {}), monitors={}",
            context,
            window_left,
            window_top,
            window_right,
            window_bottom,
            monitors.len()
        );
        trace_err!(window.center(), "recenter offscreen main window");
    }
}

#[cfg(target_os = "windows")]
fn force_activate_window(window: &tauri::Window, context: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, SetForegroundWindow, ShowWindowAsync, SW_RESTORE,
    };

    match window.hwnd() {
        Ok(hwnd) => {
            let hwnd = hwnd.0 as isize;
            log::trace!("windows force activate executing, context={}", context);
            unsafe {
                let show_ok = ShowWindowAsync(hwnd, SW_RESTORE) != 0;
                let bring_ok = BringWindowToTop(hwnd) != 0;
                let foreground_ok = SetForegroundWindow(hwnd) != 0;
                log::trace!(
                    "windows force activate finished, context={}, show_ok={}, bring_ok={}, foreground_ok={}",
                    context,
                    show_ok,
                    bring_ok,
                    foreground_ok
                );
                if !foreground_ok {
                    log::trace!(
                        "windows force activate SetForegroundWindow returned false, context={} (Windows foreground restrictions may apply)",
                        context
                    );
                }
            }
        }
        Err(err) => {
            log::trace!(
                "windows force activate skipped because hwnd unavailable, context={}, error={}",
                context,
                err
            );
        }
    }
}

fn window_state(window: &tauri::Window) -> (bool, bool, bool) {
    let visible = window.is_visible().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    let focused = window.is_focused().unwrap_or(false);
    (visible, minimized, focused)
}

#[cfg(target_os = "windows")]
fn window_needs_repair(window: &tauri::Window) -> bool {
    let (visible, minimized, focused) = window_state(window);
    if !focused {
        log::trace!("windows window focus is false but will not trigger repair by itself");
    }
    !visible || minimized
}

pub fn focus_main_window_if_open(app_handle: &AppHandle, context: &str) -> bool {
    let Some(window) = app_handle.get_window("main") else {
        return false;
    };

    let (visible, minimized, focused) = window_state(&window);
    if !visible || minimized {
        return false;
    }

    log::trace!(
        "main window lightweight focus requested, context={}, visible={}, minimized={}, focused={}",
        context,
        visible,
        minimized,
        focused
    );
    trace_err!(window.set_focus(), "set win focus lightweight");

    #[cfg(target_os = "windows")]
    force_activate_window(&window, context);

    true
}

fn record_pending_show_request(request_id: u64) {
    let mut pending = MAIN_WINDOW_SHOW_PENDING_ID.load(Ordering::SeqCst);
    while request_id > pending {
        match MAIN_WINDOW_SHOW_PENDING_ID.compare_exchange(
            pending,
            request_id,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => return,
            Err(next_pending) => pending = next_pending,
        }
    }
}

fn window_show_once_fast(window: &tauri::Window, request_id: u64, reason: ShowReason) {
    log::trace!(
        "main window fast show attempt, request_id={}, reason={:?}",
        request_id,
        reason
    );
    trace_err!(window.unminimize(), "set win unminimize");
    trace_err!(window.show(), "set win visible");
    trace_err!(window.set_focus(), "set win focus");
    ensure_main_window_onscreen(window, "fast show after show");

    #[cfg(target_os = "windows")]
    force_activate_window(window, "fast show explicit");
}

fn window_show_once_repair(window: &tauri::Window, request_id: u64, context: &str) {
    log::trace!(
        "main window repair show attempt, request_id={}, context={}",
        request_id,
        context
    );
    ensure_main_window_onscreen(window, &format!("{} before show", context));
    trace_err!(window.unminimize(), "set win unminimize");
    trace_err!(window.show(), "set win visible");
    trace_err!(window.set_focus(), "set win focus");
    ensure_main_window_onscreen(window, &format!("{} after show", context));

    #[cfg(target_os = "windows")]
    force_activate_window(window, context);
}

fn finish_show_flow(
    window: &tauri::Window,
    current_request_id: u64,
    showing_guard: MainWindowShowingGuard,
) {
    drop(showing_guard);
    log_window_state(window, "show_existing_window released final");

    let pending_request_id = MAIN_WINDOW_SHOW_PENDING_ID.load(Ordering::SeqCst);
    if pending_request_id <= current_request_id {
        return;
    }

    log::trace!(
        "show flow processing merged pending request, current_request_id={}, pending_request_id={}",
        current_request_id,
        pending_request_id
    );
    request_show_existing_window(window, pending_request_id, ShowReason::Explicit);
}

#[cfg(target_os = "windows")]
fn schedule_windows_show_retries(
    window: tauri::Window,
    request_id: u64,
    reason: ShowReason,
    showing_guard: MainWindowShowingGuard,
) {
    log::trace!(
        "windows delayed retry scheduled, request_id={}, reason={:?}",
        request_id,
        reason
    );
    let showing_guard = Arc::new(std::sync::Mutex::new(Some(showing_guard)));
    let run: CancellableJobFn = Arc::new(move |cancel_token| {
        let window = window.clone();
        let showing_guard = showing_guard.clone();
        let future: JobFuture = Box::pin(async move {
            log::debug!(
                target: "app",
                "windows delayed retry task fired, request_id={}, reason={:?}",
                request_id,
                reason
            );
            let Some(showing_guard) = showing_guard.lock().unwrap().take() else {
                log::error!(
                    target: "app",
                    "windows delayed retry task missing showing guard, request_id={}",
                    request_id
                );
                return Ok(());
            };

            let retries = [(1usize, 200u64), (2, 600)];
            for (attempt, delay_ms) in retries {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                    _ = cancel_token.cancelled() => {
                        log::debug!(
                            target: "app",
                            "windows delayed retry task cancelled before attempt, request_id={}, attempt={}",
                            request_id,
                            attempt
                        );
                        return Ok(());
                    }
                }

                if cancel_token.is_cancelled() {
                    log::debug!(
                        target: "app",
                        "windows delayed retry task cancelled after wait, request_id={}, attempt={}",
                        request_id,
                        attempt
                    );
                    return Ok(());
                }

                let latest_request_id = MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst);
                if latest_request_id != request_id {
                    log::trace!(
                        "windows delayed retry skipped because stale, request_id={}, latest_request_id={}, attempt={}, delay_ms={}",
                        request_id,
                        latest_request_id,
                        attempt,
                        delay_ms
                    );
                    break;
                }

                let (visible, minimized, focused) = window_state(&window);
                if !window_needs_repair(&window) {
                    log::trace!(
                        "windows delayed retry skipped because window already visible, request_id={}, attempt={}, visible={}, minimized={}, focused={}",
                        request_id,
                        attempt,
                        visible,
                        minimized,
                        focused
                    );
                    log_window_state(&window, "windows delayed retry final healthy");
                    break;
                }

                window_show_once_repair(
                    &window,
                    request_id,
                    &format!("windows delayed {}ms", delay_ms),
                );

                if attempt == 2 {
                    log_window_state(&window, "windows delayed retry final");
                }
            }

            finish_show_flow(&window, request_id, showing_guard);
            Ok(())
        });
        future
    });

    if let Err(err) = OneShotScheduler::global().schedule_once_cancellable(
        WINDOW_SHOW_RETRIES_TASK,
        Duration::from_millis(WINDOW_SHOW_RETRIES_DELAY_MS),
        run,
    ) {
        log::error!(
            target: "app",
            "failed to schedule windows delayed retry: {err}"
        );
    } else {
        log::debug!(
            target: "app",
            "windows delayed retry task scheduled, delay_ms={}, key={WINDOW_SHOW_RETRIES_TASK}",
            WINDOW_SHOW_RETRIES_DELAY_MS
        );
    }
}

fn request_show_existing_window(window: &tauri::Window, request_id: u64, reason: ShowReason) {
    record_pending_show_request(request_id);

    let Some(showing_guard) = MainWindowShowingGuard::try_lock(request_id, reason) else {
        return;
    };

    let current_request_id = MAIN_WINDOW_SHOW_PENDING_ID.swap(0, Ordering::SeqCst);
    window_show_once_fast(window, current_request_id, reason);

    #[cfg(target_os = "windows")]
    schedule_windows_show_retries(window.clone(), current_request_id, reason, showing_guard);
}

pub(crate) fn show_existing_window(window: &tauri::Window, request_id: u64, reason: ShowReason) {
    request_show_existing_window(window, request_id, reason);
}

pub fn show_main_window_after_hide_transition(app_handle: AppHandle) {
    let scheduler = OneShotScheduler::global();
    if !is_window_hiding_in_progress() {
        scheduler.cancel(WINDOW_SHOW_AFTER_HIDE_TASK);
        show_main_window(&app_handle);
        return;
    }

    log::warn!(
        target: "app",
        "show_main_window delayed because window hide is in progress"
    );
    let run: JobFn = Arc::new(move || {
        let app_handle = app_handle.clone();
        let future: JobFuture = Box::pin(async move {
            log::debug!(target: "app", "show_main_window delayed task fired");
            show_main_window(&app_handle);
            Ok(())
        });
        future
    });

    if let Err(err) = scheduler.schedule_once(
        WINDOW_SHOW_AFTER_HIDE_TASK,
        Duration::from_millis(WINDOW_SHOW_AFTER_HIDE_DELAY_MS),
        run,
    ) {
        log::error!(
            target: "app",
            "failed to schedule delayed show_main_window: {err}"
        );
    } else {
        log::debug!(
            target: "app",
            "show_main_window delayed task scheduled, delay_ms={}, key={WINDOW_SHOW_AFTER_HIDE_TASK}",
            WINDOW_SHOW_AFTER_HIDE_DELAY_MS
        );
    }
}

/// show and focus the main window
pub fn show_main_window(app_handle: &AppHandle) {
    check_main_window_health(app_handle, "show_main_window");
    let show_request_id = MAIN_WINDOW_SHOW_REQUEST_ID.fetch_add(1, Ordering::SeqCst) + 1;
    FRONTEND_SHOW_WHEN_READY.store(true, Ordering::SeqCst);
    let existing = app_handle.get_window("main");
    log::trace!(
        "show_main_window called, existing window {}, show_request_id={}",
        if existing.is_some() { "yes" } else { "no" },
        show_request_id
    );

    if let Some(window) = existing {
        show_existing_window(&window, show_request_id, ShowReason::Explicit);
    } else {
        record_pending_show_request(show_request_id);
        let now = current_time_millis();
        if !main_window_recreate_allowed(now) {
            return;
        }
        log::trace!("show_main_window did not find main window, create fallback");
        create_window(app_handle, true);
    }
}
