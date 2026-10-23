//! 后台定时任务调度器。
//!
//! 所有周期任务统一以稳定的 key 注册到 [`Scheduler`]，由调度器负责创建、
//! 更新和移除底层的 `delay_timer` 任务。应用启动时调用 [`init`]，新增任务
//! 时在 `init` 中追加对应模块的注册调用即可。
//!
//! 调度器只负责任务触发；任务返回的错误会被统一记录，不会影响其他任务。

use anyhow::{bail, Context, Result};
use delay_timer::prelude::{DelayTimer, DelayTimerBuilder, Task, TaskBuilder};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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

    /// 返回指定前缀下已注册的任务 key。
    pub fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        let jobs = self.jobs.lock();
        jobs.keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect()
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

    let task_count = Scheduler::global().keys_with_prefix("").len();
    log::info!(target: "app", "scheduler initialized with {task_count} task(s)");
    Ok(())
}
