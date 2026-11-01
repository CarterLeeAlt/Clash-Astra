use super::state::*;
use crate::core::scheduler::{JobFn, JobFuture, OneShotScheduler};
use crate::domain::window_geometry::geometry_to_persist;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub fn schedule_window_state_save(app_handle: AppHandle, delay: Duration) {
    let key = format!("{WINDOW_STATE_SAVE_TASK_PREFIX}:{}ms", delay.as_millis());
    let run: JobFn = Arc::new(move || {
        let app_handle = app_handle.clone();
        let future: JobFuture = Box::pin(async move {
            match save_window_size_position(&app_handle) {
                Ok(_) => log::debug!(target: "app", "window state saved after delayed request"),
                Err(err) => log::trace!(
                    target: "app",
                    "window state delayed save skipped: {err}"
                ),
            }
            Ok(())
        });
        future
    });

    let scheduler = OneShotScheduler::global();
    if let Err(err) = scheduler.schedule_once(key.clone(), delay, run) {
        log::error!(
            target: "app",
            "failed to schedule window state save `{key}`: {err}"
        );
        return;
    }

    // 完整保存会读取当前窗口状态并落盘，取消尚未执行的拖动防抖保存。
    scheduler.cancel(WINDOW_SIZE_POSITION_SAVE_TASK);
    log::debug!(
        target: "app",
        "window state save task scheduled, delay_ms={}, key={key}",
        delay.as_millis()
    );
}

pub fn schedule_save_window_size_position(app_handle: AppHandle) {
    let scheduler = OneShotScheduler::global();
    let was_pending = scheduler.is_scheduled(WINDOW_SIZE_POSITION_SAVE_TASK);
    let moved_at = current_time_millis();

    let run: JobFn = Arc::new(move || {
        let app_handle = app_handle.clone();
        let future: JobFuture = Box::pin(async move {
            if is_window_hiding_in_progress() {
                log::debug!(
                    target: "app",
                    "window moved/resized save skipped, reason=hiding_or_not_visible"
                );
                return Ok(());
            }

            let Some(window) = app_handle.get_window("main") else {
                log::debug!(
                    target: "app",
                    "window moved/resized save skipped, reason=hiding_or_not_visible"
                );
                return Ok(());
            };

            if !window.is_visible().unwrap_or(false) {
                log::debug!(
                    target: "app",
                    "window moved/resized save skipped, reason=hiding_or_not_visible"
                );
                return Ok(());
            }

            let elapsed = current_time_millis().saturating_sub(moved_at);
            match save_window_size_position(&app_handle) {
                Ok(_) => log::debug!(
                    target: "app",
                    "window moved/resized saved after quiet, elapsed_ms={elapsed}"
                ),
                Err(err) => log::trace!(
                    target: "app",
                    "window moved/resized save after quiet skipped: {err}"
                ),
            }
            Ok(())
        });
        future
    });

    if let Err(err) = scheduler.debounce(
        WINDOW_SIZE_POSITION_SAVE_TASK,
        Duration::from_millis(WINDOW_SAVE_DEBOUNCE_MS),
        run,
    ) {
        log::error!(
            target: "app",
            "failed to schedule window size/position save: {err}"
        );
    } else if !was_pending {
        log::debug!(target: "app", "window moved/resized debounce started");
    }
}

/// 采集逻辑坐标下的窗口几何信息，过程中不持有配置写锁。
pub(crate) fn read_window_state(app_handle: &AppHandle) -> Result<(Option<Vec<f64>>, bool)> {
    let win = app_handle
        .get_window("main")
        .ok_or(anyhow::anyhow!("failed to get window"))?;

    // 窗口 getter 会同步派发到主线程并等待结果；必须在拿配置锁之前取完，
    // 否则主线程上的同步命令（如 get_clash_settings）等锁时会与这里互锁。
    let scale = win.scale_factor()?;
    let size = win.inner_size()?;
    let size = size.to_logical::<f64>(scale);
    let pos = win.outer_position()?;
    let pos = pos.to_logical::<f64>(scale);
    let is_maximized = win.is_maximized()?;

    let position = geometry_to_persist(size.width, size.height, pos.x, pos.y, is_maximized);
    Ok((position, is_maximized))
}

/// 窗口事件只采集几何状态，写盘排入应用协调器，避免阻塞主线程。
pub fn save_window_size_position(app_handle: &AppHandle) -> Result<()> {
    let (position, maximized) = read_window_state(app_handle)?;
    crate::application::configuration::save_window_state(position, maximized);
    Ok(())
}

/// 退出工作线程等待窗口状态写入结束后，再关闭业务入口和外部资源。
pub fn save_window_state_before_shutdown(app_handle: &AppHandle) -> Result<()> {
    let (position, maximized) = read_window_state(app_handle)?;
    tauri::async_runtime::block_on(crate::application::configuration::persist_window_state(
        position, maximized,
    ))
}
