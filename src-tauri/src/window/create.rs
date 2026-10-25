use super::show::show_existing_window;
use super::state::*;
use crate::config::{Config, IVerge};
use crate::core::scheduler::{JobFn, JobFuture, OneShotScheduler};
use crate::trace_err;
use crate::utils::dirs;
use anyhow::Result;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use window_shadows::set_shadow;

fn schedule_create_window_show_fallback(
    app_handle: AppHandle,
    show_when_ready: bool,
    scheduled_show_request_id: u64,
) {
    log::trace!(
        "create_window show fallback scheduled, show_when_ready={}, show_request_id={}",
        show_when_ready,
        scheduled_show_request_id
    );
    let run: JobFn = Arc::new(move || {
        let app_handle = app_handle.clone();
        let future: JobFuture = Box::pin(async move {
            log::debug!(target: "app", "create_window show fallback task fired");
            let ready_handled = FRONTEND_READY_HANDLED.load(Ordering::SeqCst);
            let current_show_request_id = MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst);
            let should_show = show_when_ready || current_show_request_id > 0;

            if !should_show {
                log::trace!(
                    "create_window show fallback skipped, silent without show request, ready_handled={}, show_request_id={}",
                    ready_handled,
                    current_show_request_id
                );
                return Ok(());
            }

            let Some(window) = app_handle.get_window("main") else {
                log::trace!(
                    "create_window show fallback skipped, main window missing, ready_handled={}, show_request_id={}",
                    ready_handled,
                    current_show_request_id
                );
                return Ok(());
            };

            let visible = window.is_visible().unwrap_or(false);
            let minimized = window.is_minimized().unwrap_or(false);
            if !ready_handled
                || current_show_request_id > scheduled_show_request_id
                || !visible
                || minimized
            {
                log::trace!(
                    "create_window show fallback triggered, ready_handled={}, visible={}, minimized={}, show_when_ready={}, scheduled_show_request_id={}, current_show_request_id={}",
                    ready_handled,
                    visible,
                    minimized,
                    show_when_ready,
                    scheduled_show_request_id,
                    current_show_request_id
                );
                show_existing_window(&window, current_show_request_id, ShowReason::CreateFallback);
            } else {
                log::trace!(
                    "create_window show fallback skipped, ready already showed window, ready_handled={}, visible={}, minimized={}, show_request_id={}",
                    ready_handled,
                    visible,
                    minimized,
                    current_show_request_id
                );
            }
            Ok(())
        });
        future
    });

    if let Err(err) = OneShotScheduler::global().schedule_once(
        WINDOW_SHOW_CREATE_FALLBACK_TASK,
        Duration::from_millis(WINDOW_SHOW_CREATE_FALLBACK_DELAY_MS),
        run,
    ) {
        log::error!(
            target: "app",
            "failed to schedule create_window show fallback: {err}"
        );
    } else {
        log::debug!(
            target: "app",
            "create_window show fallback task scheduled, delay_ms={}, key={WINDOW_SHOW_CREATE_FALLBACK_TASK}",
            WINDOW_SHOW_CREATE_FALLBACK_DELAY_MS
        );
    }
}

#[cfg(target_os = "windows")]
fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn valid_window_size_position(size_pos: &[f64]) -> bool {
    size_pos.len() == 4
        && size_pos.iter().all(|value| value.is_finite())
        && size_pos[0] >= 1060.0
        && size_pos[1] >= 680.0
        && size_pos[0] <= 20_000.0
        && size_pos[1] <= 20_000.0
        && size_pos[2].abs() <= 100_000.0
        && size_pos[3].abs() <= 100_000.0
}

#[cfg(target_os = "windows")]
fn windows_webview2_args(reliable_mode: bool, fallback_mode: bool) -> Option<String> {
    let mut args = Vec::new();

    if !reliable_mode {
        args.push("--enable-features=msWebView2EnableDraggableRegions".to_string());
    }

    if fallback_mode || env_flag_enabled("CLASH_VERGE_WEBVIEW2_DISABLE_GPU") {
        args.push("--disable-gpu".to_string());
    }

    if fallback_mode || env_flag_enabled("CLASH_VERGE_WEBVIEW2_DISABLE_HW_ACCELERATION") {
        args.push("--disable-software-rasterizer".to_string());
        args.push("--disable-accelerated-2d-canvas".to_string());
        args.push("--disable-accelerated-video-decode".to_string());
    }

    if let Ok(extra_args) = std::env::var("CLASH_VERGE_WEBVIEW2_ARGS") {
        args.extend(
            extra_args
                .split_whitespace()
                .filter(|arg| !arg.trim().is_empty())
                .map(|arg| arg.to_string()),
        );
    }

    if args.is_empty() {
        None
    } else {
        Some(args.join(" "))
    }
}

/// create main window
pub fn create_window(app_handle: &AppHandle, show_when_ready: bool) {
    if show_when_ready {
        FRONTEND_SHOW_WHEN_READY.store(true, Ordering::SeqCst);
    }

    let now = current_time_millis();
    if !main_window_recreate_allowed(now) {
        return;
    }
    let build_mode = if should_force_reliable_fallback(now) {
        WindowBuildMode::ForcedReliableFallback
    } else {
        WindowBuildMode::NormalConfigured
    };
    let reliable_mode = effective_reliable_mode(build_mode);
    MAIN_WINDOW_LAST_CREATE_ATTEMPT_MS.store(now, Ordering::SeqCst);

    log::trace!(
        "create_window called, show_when_ready={}, existing={}, show_request_id={}",
        show_when_ready,
        app_handle.get_window("main").is_some(),
        MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst)
    );
    if reliable_mode {
        log::info!(
            target: "app",
            "create_window using reliable mode, mode={:?}, reason={}, transparent=false, decorations=true, set_shadow skipped, draggable regions disabled",
            build_mode,
            build_mode.reason()
        );
    } else {
        log::warn!(
            target: "app",
            "create_window using experimental custom frameless mode, mode={:?}, reason={}, transparent=true, decorations=false, set_shadow enabled, draggable regions enabled",
            build_mode,
            build_mode.reason()
        );
    }

    if let Some(window) = app_handle.get_window("main") {
        MAIN_WINDOW_STATE.store(MAIN_WINDOW_STATE_ALIVE, Ordering::SeqCst);
        let show_request_id = MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst);
        if show_when_ready || show_request_id > 0 {
            log::trace!(
                "create_window found existing main window, show/focus it, show_request_id={}",
                show_request_id
            );
            show_existing_window(&window, show_request_id, ShowReason::CreateExisting);
        } else {
            log::trace!("create_window found existing main window, leave visibility unchanged");
        }
        return;
    }

    let Some(_creating_guard) = MainWindowCreatingGuard::try_lock() else {
        return;
    };
    FRONTEND_READY_HANDLED.store(false, Ordering::SeqCst);
    log::trace!("create_window reset frontend ready handled for new main window");

    let webview_data_dir = match dirs::webview_data_dir() {
        Ok(path) => path,
        Err(err) => {
            log::error!(target: "app", "failed to resolve portable WebView data directory: {err}");
            return;
        }
    };
    if let Err(err) = std::fs::create_dir_all(&webview_data_dir) {
        log::error!(target: "app", "failed to create portable WebView data directory {}: {err}", webview_data_dir.display());
        return;
    }

    let window_size_locked = Config::verge().latest().window_size_locked.unwrap_or(false);
    let configured_size_pos = Config::verge().latest().window_size_position.clone();
    let size_pos_is_valid = configured_size_pos
        .as_ref()
        .map(|size_pos| valid_window_size_position(size_pos))
        .unwrap_or(false);
    if let Some(size_pos) = configured_size_pos.as_ref() {
        if !size_pos_is_valid {
            log::warn!(
                target: "app",
                "create_window ignoring abnormal window_size_position={:?}; safe centered defaults will be used and saved value will be cleared",
                size_pos
            );
            Config::verge().data().patch_config(IVerge {
                window_size_position: None,
                ..IVerge::default()
            });
            match Config::save_verge_snapshot() {
                Ok(_) => log::info!(
                    target: "app",
                    "create_window cleared abnormal saved window_size_position"
                ),
                Err(err) => log::warn!(
                    target: "app",
                    "create_window failed to clear abnormal saved window_size_position: {err}"
                ),
            }
        } else {
            log::info!(
                target: "app",
                "create_window using saved window_size_position={:?}",
                size_pos
            );
        }
    } else {
        log::info!(target: "app", "create_window no saved window_size_position");
    }

    let elevated = crate::core::win_service::is_current_process_elevated().unwrap_or_else(|err| {
        log::warn!(target: "app", "create_window failed to detect elevation: {err}");
        false
    });
    let window_title = if elevated {
        "Clash Astra [Admin Mode]"
    } else {
        "Clash Astra [Normal Mode]"
    };
    log::info!(target: "app", "create_window title: {window_title}");

    let build_window = |fallback_mode: bool, use_saved_size_pos: bool| {
        let mut builder = tauri::window::WindowBuilder::new(
            app_handle,
            "main".to_string(),
            tauri::WindowUrl::App("index.html".into()),
        )
        .title(window_title)
        .data_directory(webview_data_dir.clone())
        .visible(false)
        .fullscreen(false)
        .min_inner_size(1060.0, 680.0)
        .resizable(!window_size_locked)
        .maximizable(!window_size_locked);

        if use_saved_size_pos {
            if let Some(size_pos) = configured_size_pos.as_ref() {
                let w = size_pos[0].clamp(1060.0, 20_000.0);
                let h = size_pos[1].clamp(680.0, 20_000.0);
                builder = builder.inner_size(w, h).position(size_pos[2], size_pos[3]);
            }
        } else {
            #[cfg(target_os = "windows")]
            {
                builder = builder.inner_size(1060.0, 680.0).center();
            }
        }

        #[cfg(target_os = "windows")]
        if let Some(args) = windows_webview2_args(reliable_mode, fallback_mode) {
            log::info!(
                target: "app",
                "create_window applying WebView2 additional_browser_args='{}', fallback_mode={}",
                args,
                fallback_mode
            );
            builder = builder.additional_browser_args(&args);
        }

        #[cfg(target_os = "windows")]
        let window = if reliable_mode || fallback_mode {
            builder
                .decorations(true)
                .transparent(false)
                .visible(false)
                .build()
        } else {
            builder
                .decorations(false)
                .transparent(true)
                .visible(false)
                .build()
        };

        window
    };

    let use_saved_size_pos = size_pos_is_valid;
    let build_start = Instant::now();
    log::info!(
        target: "app",
        "create_window primary build start before WindowBuilder::build, show_when_ready={}, show_request_id={}, reliable_mode={}, saved_size_pos={}, fallback_mode=false, window_size_locked={}",
        show_when_ready,
        MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst),
        reliable_mode,
        use_saved_size_pos,
        window_size_locked
    );
    let mut window = build_window(false, use_saved_size_pos);
    let mut used_window_fallback = false;

    log::info!(
        target: "app",
        "create_window primary build finished after WindowBuilder::build, result={}, elapsed_ms={}, show_when_ready={}, show_request_id={}, fallback_mode=false",
        if window.is_ok() { "ok" } else { "err" },
        build_start.elapsed().as_millis(),
        show_when_ready,
        MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst)
    );

    #[cfg(target_os = "windows")]
    if let Err(err) = &window {
        let retry_start = Instant::now();
        log::error!(
            target: "app",
            "create_window primary build Err: {err}; retrying with reliable decorated WebView2 fallback, disabled GPU/hardware acceleration, and centered safe size"
        );
        if configured_size_pos.is_some() && size_pos_is_valid {
            Config::verge().data().patch_config(IVerge {
                window_size_position: None,
                ..IVerge::default()
            });
            match Config::save_verge_snapshot() {
                Ok(_) => log::info!(
                    target: "app",
                    "create_window cleared saved window_size_position before fallback retry"
                ),
                Err(err) => log::warn!(
                    target: "app",
                    "create_window failed to clear saved window_size_position before fallback retry: {err}"
                ),
            }
        }
        log::info!(
            target: "app",
            "create_window fallback build start before WindowBuilder::build, show_when_ready={}, show_request_id={}, reliable_mode=true, saved_size_pos=false, fallback_mode=true",
            show_when_ready,
            MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst)
        );
        window = build_window(true, false);
        used_window_fallback = window.is_ok();
        log::info!(
            target: "app",
            "create_window fallback build finished after WindowBuilder::build, result={}, elapsed_ms={}, show_when_ready={}, show_request_id={}, fallback_mode=true",
            if window.is_ok() { "ok" } else { "err" },
            retry_start.elapsed().as_millis(),
            show_when_ready,
            MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst)
        );
    }

    match window {
        Ok(win) => {
            MAIN_WINDOW_STATE.store(MAIN_WINDOW_STATE_ALIVE, Ordering::SeqCst);
            #[cfg(target_os = "windows")]
            super::webview_recovery::install(&win);
            trace_err!(win.set_resizable(!window_size_locked), "set win resizable");
            trace_err!(
                win.set_maximizable(!window_size_locked),
                "set win maximizable"
            );
            let is_maximized = !window_size_locked
                && Config::verge()
                    .latest()
                    .window_is_maximized
                    .unwrap_or(false);
            log::trace!("try to calculate the monitor size");
            let center = (|| -> Result<bool> {
                let mut center = false;
                let monitor = win.current_monitor()?.ok_or(anyhow::anyhow!(""))?;
                let size = monitor.size();
                let pos = win.outer_position()?;

                if pos.x < -400
                    || pos.x > (size.width - 200) as i32
                    || pos.y < -200
                    || pos.y > (size.height - 200) as i32
                {
                    center = true;
                }
                Ok(center)
            })();

            if center.unwrap_or(true) {
                trace_err!(win.center(), "set win center");
            }

            if reliable_mode || used_window_fallback {
                log::info!(
                    target: "app",
                    "create_window set_shadow skipped, reliable_mode={}, fallback_mode={}",
                    reliable_mode,
                    used_window_fallback
                );
            } else {
                log::debug!(target: "app", "create_window before set_shadow");
                trace_err!(set_shadow(&win, true), "set win shadow");
                log::debug!(target: "app", "create_window after set_shadow");
            }
            if is_maximized {
                trace_err!(win.maximize(), "set win maximize");
            }

            let show_request_id = MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst);
            if show_when_ready || show_request_id > 0 {
                schedule_create_window_show_fallback(
                    app_handle.clone(),
                    show_when_ready,
                    show_request_id,
                );
            } else {
                log::trace!(
                    "create_window show fallback skipped, show_when_ready=false and no show request"
                );
            }
        }
        Err(err) => {
            MAIN_WINDOW_STATE.store(MAIN_WINDOW_STATE_MISSING, Ordering::SeqCst);
            log::error!(
                target: "app",
                "failed to create window: {err}, show_when_ready={}, show_request_id={}",
                show_when_ready,
                MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst)
            );
            return;
        }
    }

    log::trace!(
        "create_window finished, show_when_ready={}, show_request_id={}",
        show_when_ready,
        MAIN_WINDOW_SHOW_REQUEST_ID.load(Ordering::SeqCst)
    );
}
