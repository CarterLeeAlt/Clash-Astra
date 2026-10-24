use anyhow::{bail, Context, Result};
use delay_timer::prelude::{DelayTimer, DelayTimerBuilder, Task, TaskBuilder};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{JobFn, Schedule};

/// 周期任务快照。
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodicTaskInfo {
    pub key: String,
    pub schedule: Schedule,
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
        self.install(key, schedule, run)
    }

    fn install(&self, key: impl Into<String>, schedule: Schedule, run: JobFn) -> Result<()> {
        let key = key.into();
        let schedule = schedule.validate()?;

        let mut jobs = self.jobs.lock();
        if let Some(entry) = jobs.get(&key) {
            if entry.schedule == schedule {
                log::trace!(target: "app", "scheduler task `{key}` unchanged");
                return Ok(());
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
    #[cfg(test)]
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
    ///
    /// 不要在周期任务的回调内部调用本方法：`delay_timer` 会先删除任务标记，
    /// 回调结束时再执行 `finish_task`，从而打印“Without the task_mark_ref_mut”
    /// 错误。需要自行停止的周期任务应改用一次性调度器并停止续约，或在任务
    /// 外部调用本方法。
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

    /// 返回所有已注册的周期任务快照，按 key 字典序排列。
    pub fn tasks(&self) -> Vec<PeriodicTaskInfo> {
        let jobs = self.jobs.lock();
        let mut tasks: Vec<PeriodicTaskInfo> = jobs
            .iter()
            .map(|(key, entry)| PeriodicTaskInfo {
                key: key.clone(),
                schedule: entry.schedule.clone(),
            })
            .collect();
        tasks.sort_by(|left, right| left.key.cmp(&right.key));
        tasks
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

#[cfg(test)]
mod tests {
    use super::super::{test_lock, JobFn, Schedule};
    use super::Scheduler;
    use delay_timer::prelude::DelayTimerBuilder;
    use std::collections::HashMap;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    fn test_scheduler() -> Scheduler {
        Scheduler {
            delay_timer: parking_lot::Mutex::new(DelayTimerBuilder::default().build()),
            jobs: parking_lot::Mutex::new(HashMap::new()),
            next_task_id: AtomicU64::new(1),
        }
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
