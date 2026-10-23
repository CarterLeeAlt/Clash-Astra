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

use anyhow::{bail, Context, Result};
use delay_timer::prelude::{DelayTimer, DelayTimerBuilder, Task, TaskBuilder};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tokio::sync::Notify;

use crate::core::timer;

pub type JobFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
pub type JobFn = Arc<dyn Fn() -> JobFuture + Send + Sync>;

/// 任务周期。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Schedule {
    /// 按秒重复。
    Seconds(u64),
    /// 按分钟重复。
    Minutes(u64),
    /// 按 cron 表达式重复，使用本地时区。
    #[allow(dead_code)]
    Cron(String),
}

struct JobEntry {
    task_id: u64,
    schedule: Schedule,
}

/// 全局任务调度器。
pub struct Scheduler {
    delay_timer: Mutex<DelayTimer>,
    jobs: Mutex<HashMap<String, JobEntry>>,
    next_task_id: AtomicU64,
}

impl Scheduler {
    pub fn global() -> &'static Scheduler {
        static SCHEDULER: OnceCell<Scheduler> = OnceCell::new();

        SCHEDULER.get_or_init(|| Scheduler {
            delay_timer: Mutex::new(DelayTimerBuilder::default().build()),
            jobs: Mutex::new(HashMap::new()),
            next_task_id: AtomicU64::new(1),
        })
    }

    /// 注册或更新一个周期任务。
    ///
    /// 同一个 key 的周期没有变化时保留原任务，不会重置计时；因此任务的运行
    /// 逻辑应从最新配置读取状态，不要依赖注册时捕获的可变快照。
    pub fn upsert(&self, key: impl Into<String>, schedule: Schedule, run: JobFn) -> Result<()> {
        self.install(key, schedule, run, false)
    }

    /// 强制替换任务并重置计时，即使周期没有变化。
    ///
    /// 适合需要在每次执行结束后重新计算下一次触发时间的任务。
    pub fn replace(&self, key: impl Into<String>, schedule: Schedule, run: JobFn) -> Result<()> {
        self.install(key, schedule, run, true)
    }

    fn install(
        &self,
        key: impl Into<String>,
        schedule: Schedule,
        run: JobFn,
        force: bool,
    ) -> Result<()> {
        let key = key.into();
        let schedule = schedule.validate()?;

        let mut jobs = self.jobs.lock();
        if !force {
            if let Some(entry) = jobs.get(&key) {
                if entry.schedule == schedule {
                    log::trace!(target: "app", "scheduler task `{key}` unchanged");
                    return Ok(());
                }
            }
        }

        let task_id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        let task = self.build_task(task_id, &key, &schedule, run)?;

        let delay_timer = self.delay_timer.lock();
        delay_timer
            .add_task(task)
            .with_context(|| format!("failed to add scheduled task `{key}`"))?;

        if let Some(entry) = jobs.insert(key.clone(), JobEntry { task_id, schedule }) {
            crate::log_err!(delay_timer.remove_task(entry.task_id));
        }

        log::debug!(target: "app", "scheduler registered task `{key}`");
        Ok(())
    }

    /// 任务是否已注册。
    pub fn is_registered(&self, key: &str) -> bool {
        self.jobs.lock().contains_key(key)
    }

    /// 停止并移除所有周期任务。
    ///
    /// 正在执行的任务不会被强制中断；此方法只保证之后不会再触发。
    pub fn shutdown(&self) -> Result<()> {
        let tasks: Vec<(String, u64)> = self
            .jobs
            .lock()
            .drain()
            .map(|(key, entry)| (key, entry.task_id))
            .collect();

        if tasks.is_empty() {
            log::debug!(target: "app", "scheduler shutdown: no tasks");
            return Ok(());
        }

        let keys: Vec<&str> = tasks.iter().map(|(key, _)| key.as_str()).collect();
        let delay_timer = self.delay_timer.lock();
        let mut failures = Vec::new();
        for (key, task_id) in &tasks {
            if let Err(err) = delay_timer.remove_task(*task_id) {
                log::error!(
                    target: "app",
                    "scheduler shutdown failed to remove task `{key}`: {err}"
                );
                failures.push(key.clone());
            }
        }
        drop(delay_timer);

        log::info!(
            target: "app",
            "scheduler shutdown removed {} task(s): {keys:?}",
            tasks.len()
        );
        if !failures.is_empty() {
            bail!(
                "failed to remove {} scheduled task(s) during shutdown: {failures:?}",
                failures.len()
            );
        }
        Ok(())
    }

    /// 移除任务；任务不存在时直接返回。
    pub fn remove(&self, key: &str) -> Result<()> {
        let mut jobs = self.jobs.lock();
        let Some(entry) = jobs.remove(key) else {
            return Ok(());
        };

        let delay_timer = self.delay_timer.lock();
        delay_timer
            .remove_task(entry.task_id)
            .with_context(|| format!("failed to remove scheduled task `{key}`"))?;

        log::debug!(target: "app", "scheduler removed task `{key}`");
        Ok(())
    }

    /// 立即触发一次任务；返回任务是否存在。
    pub fn trigger(&self, key: &str) -> Result<bool> {
        let jobs = self.jobs.lock();
        let Some(entry) = jobs.get(key) else {
            return Ok(false);
        };

        let delay_timer = self.delay_timer.lock();
        delay_timer
            .advance_task(entry.task_id)
            .with_context(|| format!("failed to trigger scheduled task `{key}`"))?;

        log::debug!(target: "app", "scheduler triggered task `{key}`");
        Ok(true)
    }

    /// 返回所有已注册的周期任务 key，按字典序排列。
    pub fn keys(&self) -> Vec<String> {
        self.keys_with_prefix("")
    }

    /// 返回指定前缀下已注册的任务 key，按字典序排列。
    pub fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        let jobs = self.jobs.lock();
        let mut keys: Vec<String> = jobs
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn build_task(&self, task_id: u64, key: &str, schedule: &Schedule, run: JobFn) -> Result<Task> {
        let mut builder = TaskBuilder::default();
        builder
            .set_task_id(task_id)
            .set_maximum_parallel_runnable_num(1);

        match schedule {
            Schedule::Seconds(seconds) => {
                builder.set_frequency_repeated_by_seconds(*seconds);
            }
            Schedule::Minutes(minutes) => {
                builder.set_frequency_repeated_by_minutes(*minutes);
            }
            Schedule::Cron(expression) => {
                builder.set_frequency_repeated_by_cron_str(expression);
            }
        }

        let key = key.to_string();
        builder
            .spawn_async_routine(move || {
                let run = run.clone();
                let key = key.clone();
                async move {
                    log::trace!(target: "app", "scheduler task `{key}` started");
                    if let Err(err) = run().await {
                        log::error!(target: "app", "scheduler task `{key}` failed: {err:#}");
                    }
                }
            })
            .context("failed to create scheduled task")
    }
}

/// 一次性任务条目。
///
/// 每个 key 只保留一个等待任务；重复调度只更新截止时间和执行函数，并唤醒
/// 等待中的 worker，避免事件密集时反复创建和取消任务。
struct OneShotEntry {
    generation: u64,
    deadline: tokio::time::Instant,
    notify: Arc<Notify>,
    run: JobFn,
    handle: JoinHandle<()>,
}

/// 一次性/防抖任务调度器。
///
/// 任务只执行一次，执行前自动从注册表移除。同一个 key 再次调度会取消尚未
/// 触发的旧任务并重新计时，因此同 key 的重复调用就是尾沿防抖，只保留最后
/// 一次。已经在执行的任务不会被取消。
pub struct OneShotScheduler {
    jobs: Mutex<HashMap<String, OneShotEntry>>,
    next_generation: AtomicU64,
}

impl OneShotScheduler {
    pub fn global() -> &'static OneShotScheduler {
        static ONE_SHOT: OnceCell<OneShotScheduler> = OnceCell::new();

        ONE_SHOT.get_or_init(|| OneShotScheduler {
            jobs: Mutex::new(HashMap::new()),
            next_generation: AtomicU64::new(1),
        })
    }

    /// 安排一次性任务；同 key 的旧任务会被取消并重新计时。
    pub fn schedule_once(&self, key: impl Into<String>, delay: Duration, run: JobFn) -> Result<()> {
        if delay.is_zero() {
            bail!("one-shot task delay must be greater than zero");
        }

        let key = key.into();
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let deadline = tokio::time::Instant::now() + delay;

        let mut jobs = self.jobs.lock();
        if let Some(entry) = jobs.get_mut(&key) {
            entry.generation = generation;
            entry.deadline = deadline;
            entry.run = run;
            entry.notify.notify_one();
            drop(jobs);

            log::trace!(target: "app", "one-shot task `{key}` rescheduled in {delay:?}");
            return Ok(());
        }

        let notify = Arc::new(Notify::new());
        let worker_key = key.clone();

        // 先持锁再 spawn，确保 worker 第一次读取时一定能看到本次条目。
        let handle = tauri::async_runtime::spawn(async move {
            run_one_shot_worker(worker_key).await;
        });
        jobs.insert(
            key.clone(),
            OneShotEntry {
                generation,
                deadline,
                notify,
                run,
                handle,
            },
        );
        drop(jobs);

        log::trace!(target: "app", "one-shot task `{key}` scheduled in {delay:?}");
        Ok(())
    }

    /// 防抖：同 key 的重复调用只保留最后一次。
    pub fn debounce(&self, key: impl Into<String>, delay: Duration, run: JobFn) -> Result<()> {
        self.schedule_once(key, delay, run)
    }

    /// 取消尚未触发的一次性任务；返回是否确实取消了任务。
    pub fn cancel(&self, key: &str) -> bool {
        let entry = self.jobs.lock().remove(key);
        let Some(entry) = entry else {
            return false;
        };

        entry.notify.notify_one();
        entry.handle.abort();
        log::debug!(target: "app", "one-shot task `{key}` cancelled");
        true
    }

    /// 取消所有尚未触发的一次性任务。
    pub fn cancel_all(&self) {
        let entries: Vec<(String, OneShotEntry)> = self.jobs.lock().drain().collect();
        if entries.is_empty() {
            return;
        }

        let keys: Vec<&str> = entries.iter().map(|(key, _)| key.as_str()).collect();
        for (_, entry) in &entries {
            entry.notify.notify_one();
            entry.handle.abort();
        }
        log::debug!(
            target: "app",
            "one-shot tasks cancelled: {} keys: {keys:?}",
            entries.len()
        );
    }

    /// 返回所有尚未触发的一次性任务 key，按字典序排列。
    pub fn pending_keys(&self) -> Vec<String> {
        let jobs = self.jobs.lock();
        let mut keys: Vec<String> = jobs.keys().cloned().collect();
        keys.sort();
        keys
    }

    /// 指定一次性任务是否仍在等待触发。
    pub fn is_scheduled(&self, key: &str) -> bool {
        self.jobs.lock().contains_key(key)
    }
}

async fn run_one_shot_worker(key: String) {
    let scheduler = OneShotScheduler::global();

    loop {
        let (generation, deadline, notify) = {
            let jobs = scheduler.jobs.lock();
            let Some(entry) = jobs.get(&key) else {
                return;
            };
            (entry.generation, entry.deadline, entry.notify.clone())
        };

        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                let run = {
                    let mut jobs = scheduler.jobs.lock();
                    let Some(entry) = jobs.get(&key) else {
                        return;
                    };
                    if entry.generation != generation {
                        continue;
                    }
                    jobs.remove(&key).map(|entry| entry.run)
                };
                let Some(run) = run else {
                    return;
                };

                log::debug!(target: "app", "one-shot task `{key}` fired");
                if let Err(err) = run().await {
                    log::error!(target: "app", "one-shot task `{key}` failed: {err:#}");
                }
                return;
            }
            _ = notify.notified() => {
                log::trace!(target: "app", "one-shot task `{key}` deadline reset");
            }
        }
    }
}

impl Schedule {
    fn validate(self) -> Result<Self> {
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

    // 后续新增周期任务时，在这里追加注册调用。

    let task_keys = Scheduler::global().keys();
    log::info!(
        target: "app",
        "scheduler initialized with {} task(s): {task_keys:?}",
        task_keys.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use delay_timer::prelude::DelayTimerBuilder;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    type Events = Arc<Mutex<Vec<&'static str>>>;
    type Values = Arc<Mutex<Vec<usize>>>;

    /// 调度器测试共享全局单例，串行执行以避免 cancel_all 互相干扰。
    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn recording_job(events: Events, value: &'static str) -> JobFn {
        Arc::new(move || {
            let events = events.clone();
            Box::pin(async move {
                events.lock().unwrap().push(value);
                Ok(())
            })
        })
    }

    fn recording_value_job(events: Values, value: usize) -> JobFn {
        Arc::new(move || {
            let events = events.clone();
            Box::pin(async move {
                events.lock().unwrap().push(value);
                Ok(())
            })
        })
    }

    fn wait_until(timeout: Duration, predicate: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        predicate()
    }

    fn test_scheduler() -> Scheduler {
        Scheduler {
            delay_timer: parking_lot::Mutex::new(DelayTimerBuilder::default().build()),
            jobs: parking_lot::Mutex::new(HashMap::new()),
            next_task_id: AtomicU64::new(1),
        }
    }

    #[test]
    fn schedule_rejects_zero_intervals_and_empty_cron() {
        let _guard = test_lock();
        assert!(Schedule::Seconds(0).validate().is_err());
        assert!(Schedule::Minutes(0).validate().is_err());
        assert!(Schedule::Cron("   ".to_string()).validate().is_err());
        assert!(Schedule::Seconds(1).validate().is_ok());
    }

    #[test]
    fn one_shot_rejects_zero_delay() {
        let _guard = test_lock();
        let events = Arc::new(Mutex::new(Vec::new()));
        let result = OneShotScheduler::global().schedule_once(
            "test.one_shot.zero_delay",
            Duration::ZERO,
            recording_job(events, "fired"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn one_shot_runs_once_and_removes_entry() {
        let _guard = test_lock();
        let events = Arc::new(Mutex::new(Vec::new()));
        let key = "test.one_shot.runs_once";
        OneShotScheduler::global()
            .schedule_once(
                key,
                Duration::from_millis(30),
                recording_job(events.clone(), "fired"),
            )
            .unwrap();

        assert!(wait_until(Duration::from_secs(2), || {
            !events.lock().unwrap().is_empty()
        }));
        assert_eq!(events.lock().unwrap().as_slice(), &["fired"]);
        assert!(!OneShotScheduler::global().is_scheduled(key));
    }

    #[test]
    fn debounce_keeps_only_the_last_run() {
        let _guard = test_lock();
        let events = Arc::new(Mutex::new(Vec::new()));
        let key = "test.one_shot.debounce";
        let scheduler = OneShotScheduler::global();

        scheduler
            .schedule_once(
                key,
                Duration::from_millis(80),
                recording_job(events.clone(), "first"),
            )
            .unwrap();
        scheduler
            .schedule_once(
                key,
                Duration::from_millis(20),
                recording_job(events.clone(), "second"),
            )
            .unwrap();

        assert!(wait_until(Duration::from_secs(2), || {
            !events.lock().unwrap().is_empty()
        }));
        thread::sleep(Duration::from_millis(120));
        assert_eq!(events.lock().unwrap().as_slice(), &["second"]);
        assert!(!scheduler.is_scheduled(key));
    }

    #[test]
    fn rapid_debounce_updates_only_run_last() {
        let _guard = test_lock();
        let events: Values = Arc::new(Mutex::new(Vec::new()));
        let key = "test.one_shot.rapid";
        let scheduler = OneShotScheduler::global();
        scheduler.cancel_all();

        for value in 0..20 {
            scheduler
                .schedule_once(
                    key,
                    Duration::from_millis(50),
                    recording_value_job(events.clone(), value),
                )
                .unwrap();
        }

        assert!(wait_until(Duration::from_secs(2), || {
            !events.lock().unwrap().is_empty()
        }));
        thread::sleep(Duration::from_millis(100));
        assert_eq!(events.lock().unwrap().as_slice(), &[19]);
        assert!(!scheduler.is_scheduled(key));
    }

    #[test]
    fn cancel_prevents_pending_run() {
        let _guard = test_lock();
        let events = Arc::new(Mutex::new(Vec::new()));
        let key = "test.one_shot.cancel";
        let scheduler = OneShotScheduler::global();

        scheduler
            .schedule_once(
                key,
                Duration::from_millis(200),
                recording_job(events.clone(), "cancelled"),
            )
            .unwrap();
        assert!(scheduler.is_scheduled(key));
        assert!(scheduler.cancel(key));
        assert!(!scheduler.is_scheduled(key));
        assert!(!scheduler.cancel(key));

        thread::sleep(Duration::from_millis(250));
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn cancel_all_clears_pending_one_shot_tasks() {
        let _guard = test_lock();
        let events = Arc::new(Mutex::new(Vec::new()));
        let scheduler = OneShotScheduler::global();
        scheduler.cancel_all();

        for key in ["test.one_shot.cancel_all.a", "test.one_shot.cancel_all.b"] {
            scheduler
                .schedule_once(
                    key,
                    Duration::from_millis(200),
                    recording_job(events.clone(), "should_not_run"),
                )
                .unwrap();
        }
        assert_eq!(
            scheduler.pending_keys(),
            vec![
                "test.one_shot.cancel_all.a".to_string(),
                "test.one_shot.cancel_all.b".to_string()
            ]
        );

        scheduler.cancel_all();
        assert!(scheduler.pending_keys().is_empty());
        thread::sleep(Duration::from_millis(250));
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn scheduler_shutdown_clears_periodic_tasks() {
        let _guard = test_lock();
        let scheduler = test_scheduler();
        let run: JobFn = Arc::new(|| Box::pin(async { Ok(()) }));

        scheduler
            .upsert(
                "test.scheduler.shutdown",
                Schedule::Seconds(3600),
                run.clone(),
            )
            .unwrap();
        assert!(scheduler.is_registered("test.scheduler.shutdown"));
        assert_eq!(
            scheduler.keys(),
            vec!["test.scheduler.shutdown".to_string()]
        );

        scheduler.shutdown().unwrap();
        assert!(scheduler.keys().is_empty());
        assert!(!scheduler.is_registered("test.scheduler.shutdown"));

        // 再次停止不应报告错误，也不应改变空注册表。
        scheduler.shutdown().unwrap();
        assert!(scheduler.keys().is_empty());
        drop(run);
    }
}
