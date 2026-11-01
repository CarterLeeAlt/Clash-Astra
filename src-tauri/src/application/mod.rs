pub mod actions;
pub mod budget;
pub mod configuration;
pub mod diagnostics;
pub mod profiles;
pub mod routing;
pub mod service;
pub mod transaction;
pub mod window;

use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, MutexGuard};

static OPERATIONS: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static STOPPING: AtomicBool = AtomicBool::new(false);

pub async fn operation() -> anyhow::Result<MutexGuard<'static, ()>> {
    let guard = if budget::Budget::current().is_some() {
        tokio::time::timeout(
            budget::remaining(budget::UPDATE_TIMEOUT)?,
            OPERATIONS.lock(),
        )
        .await
        .map_err(|_| budget::DeadlineExceeded("configuration operation queue timed out"))?
    } else {
        OPERATIONS.lock().await
    };
    anyhow::ensure!(
        !STOPPING.load(Ordering::SeqCst),
        "application is shutting down"
    );
    Ok(guard)
}

pub fn background_operation() -> Option<MutexGuard<'static, ()>> {
    if STOPPING.load(Ordering::SeqCst) {
        return None;
    }
    OPERATIONS.try_lock().ok()
}

pub fn begin_shutdown() {
    STOPPING.store(true, Ordering::SeqCst);
}

/// 退出线程等待已接受的操作完成，并持有同一写入门闩直到代理和内核清理结束。
pub fn shutdown_guard() -> MutexGuard<'static, ()> {
    begin_shutdown();
    tauri::async_runtime::block_on(OPERATIONS.lock())
}

/// 接受的修改独立于调用页面存活，页面卸载不能中断提交或补偿。
pub async fn dispatch<T: Send + 'static>(
    work: impl std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
) -> anyhow::Result<T> {
    dispatch_with_budget(budget::CONFIG_TIMEOUT, work).await
}

pub async fn dispatch_with_budget<T: Send + 'static>(
    timeout: std::time::Duration,
    work: impl std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
) -> anyhow::Result<T> {
    let budget = budget::Budget::current().unwrap_or_else(|| budget::Budget::new(timeout));
    // The worker retains the operation gate through compensation even when the caller times out.
    let worker = tokio::spawn(budget.scope(Box::pin(work)));
    tokio::time::timeout_at(budget.deadline, worker)
        .await
        .map_err(|_| {
            budget::DeadlineExceeded(
                "operation timed out; consistency cleanup may still be running",
            )
        })?
        .map_err(|error| anyhow::anyhow!("business operation failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn accepted_operation_finishes_when_its_caller_is_cancelled() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let caller = tokio::spawn(dispatch(async move {
            entered_tx.send(()).unwrap();
            release_rx.await.unwrap();
            done_tx.send(()).unwrap();
            Ok(())
        }));
        entered_rx.await.unwrap();
        caller.abort();
        release_tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), done_rx)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn operation_gate_excludes_background_writers() {
        let guard = operation().await.unwrap();
        assert!(background_operation().is_none());
        drop(guard);
        assert!(background_operation().is_some());
    }
}
