//! 后台定时任务调度器。
//!
//! [`Scheduler`] 管理固定周期的长期任务，所有周期任务以稳定的 key 注册到
//! 调度器，由它负责创建、更新和移除底层的 `delay_timer` 任务。应用启动时
//! 调用 [`init`]，新增周期任务时在 `init` 中追加对应模块的注册调用即可。
//!
//! [`OneShotScheduler`] 管理一次性任务和防抖：同一个 key 再次调度会取消尚未
//! 触发的旧任务并重新计时，任务执行前会自动从注册表移除。
//!
//! 两个调度器都只负责任务触发；任务返回的错误会被统一记录，不会影响其他任务。

use anyhow::{bail, Result};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::core::timer;

mod one_shot;
mod periodic;

pub use self::one_shot::*;
pub use self::periodic::*;

pub type JobFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
pub type JobFn = Arc<dyn Fn() -> JobFuture + Send + Sync>;

/// 任务周期。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Schedule {
    /// 按秒重复。
    #[allow(dead_code)]
    Seconds(u64),
    /// 按分钟重复。
    Minutes(u64),
    /// 按 cron 表达式重复，使用本地时区。
    #[allow(dead_code)]
    Cron(String),
}

impl Schedule {
    pub(crate) fn validate(self) -> Result<Self> {
        match &self {
            Self::Seconds(0) | Self::Minutes(0) => {
                bail!("scheduled task interval must be greater than zero");
            }
            Self::Cron(expression) if expression.trim().is_empty() => {
                bail!("scheduled task cron expression must not be empty");
            }
            _ => {}
        }

        Ok(self)
    }
}

/// 应用启动时调用，集中注册所有后台周期任务。
pub fn init() -> Result<()> {
    // 订阅自动更新：每个订阅按自己的更新间隔独立调度。
    timer::sync()?;
    timer::catch_up()?;

    // 自动节点延迟测试：每 2 分钟在后端直接驱动 mihomo 组测试 API，
    // 结果写入内核 delay history；不依赖前端窗口可见性与 WebView 调度。
    Scheduler::global().upsert(
        "proxy.auto_delay_test",
        Schedule::Minutes(2),
        Arc::new(|| Box::pin(async { crate::feat::run_auto_delay_test().await })),
    )?;

    // 后续新增周期任务时，在这里追加注册调用。

    let task_keys = Scheduler::global().keys();
    log::info!(
        target: "app",
        "scheduler initialized with {} task(s): {task_keys:?}",
        task_keys.len()
    );
    Ok(())
}

/// 调度器测试共享全局单例，串行执行以避免 cancel_all 互相干扰。
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|err| err.into_inner())
}

#[cfg(test)]
mod tests {
    use super::Schedule;

    #[test]
    fn schedule_rejects_zero_intervals_and_empty_cron() {
        let _guard = super::test_lock();
        assert!(Schedule::Seconds(0).validate().is_err());
        assert!(Schedule::Minutes(0).validate().is_err());
        assert!(Schedule::Cron("   ".to_string()).validate().is_err());
        assert!(Schedule::Seconds(1).validate().is_ok());
    }
}
