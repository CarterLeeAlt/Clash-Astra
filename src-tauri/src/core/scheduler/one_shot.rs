use anyhow::{bail, Result};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tokio::sync::Notify;

use super::{JobFn, JobFuture};

pub type CancellableJobFn = Arc<dyn Fn(CancelToken) -> JobFuture + Send + Sync>;

/// 待触发一次性任务的数量上限，防止动态 key 无限增长。
#[cfg(not(test))]
pub const MAX_PENDING_ONE_SHOT_TASKS: usize = 256;
#[cfg(test)]
pub const MAX_PENDING_ONE_SHOT_TASKS: usize = 4;

/// 一次性任务的协作式取消令牌。
///
/// 令牌只由调度器创建并传给可取消任务。任务应在等待点使用 [`CancelToken::cancelled`]
/// 或周期检查 [`CancelToken::is_cancelled`]；调度器不能强制中断已经进入用户代码的任务。
#[derive(Clone)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancelToken {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    /// 取消是否已经请求。
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// 等待取消请求；已经取消时立即返回。
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_one();
    }
}

/// 待触发的一次性任务快照。
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OneShotTaskInfo {
    pub key: String,
    pub remaining_ms: u64,
    pub generation: u64,
}

/// 正在执行的一次性任务快照。
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningTaskInfo {
    pub key: String,
    pub generation: u64,
    pub cancellable: bool,
}

enum OneShotJob {
    Plain(JobFn),
    Cancellable(CancellableJobFn),
}

/// 一次性任务条目。
///
/// 每个 key 只保留一个等待任务；重复调度只更新截止时间和执行函数，并唤醒
/// 等待中的 worker，避免事件密集时反复创建和取消任务。
struct OneShotEntry {
    generation: u64,
    deadline: tokio::time::Instant,
    notify: Arc<Notify>,
    job: OneShotJob,
    handle: JoinHandle<()>,
}

#[derive(Clone)]
struct RunningEntry {
    generation: u64,
    token: Option<CancelToken>,
}

/// 一次性/防抖任务调度器。
///
/// 任务只执行一次，执行前自动从注册表移除。同一个 key 再次调度会取消尚未
/// 触发的旧任务并重新计时，因此同 key 的重复调用就是尾沿防抖，只保留最后
/// 一次。普通任务进入执行后不会被取消；可取消任务可以通过取消令牌协作退出。
pub struct OneShotScheduler {
    jobs: Mutex<HashMap<String, OneShotEntry>>,
    running: Mutex<HashMap<String, Vec<RunningEntry>>>,
    next_generation: AtomicU64,
}

impl OneShotScheduler {
    pub fn global() -> &'static OneShotScheduler {
        static ONE_SHOT: OnceCell<OneShotScheduler> = OnceCell::new();

        ONE_SHOT.get_or_init(|| OneShotScheduler {
            jobs: Mutex::new(HashMap::new()),
            running: Mutex::new(HashMap::new()),
            next_generation: AtomicU64::new(1),
        })
    }

    /// 安排一次性任务；同 key 的旧任务会被取消并重新计时。
    pub fn schedule_once(&self, key: impl Into<String>, delay: Duration, run: JobFn) -> Result<()> {
        self.schedule_one_shot(key.into(), delay, OneShotJob::Plain(run))
    }

    /// 安排可协作取消的一次性任务。
    pub fn schedule_once_cancellable(
        &self,
        key: impl Into<String>,
        delay: Duration,
        run: CancellableJobFn,
    ) -> Result<()> {
        self.schedule_one_shot(key.into(), delay, OneShotJob::Cancellable(run))
    }

    fn schedule_one_shot(&self, key: String, delay: Duration, job: OneShotJob) -> Result<()> {
        if delay.is_zero() {
            bail!("one-shot task delay must be greater than zero");
        }

        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let deadline = tokio::time::Instant::now() + delay;

        let mut jobs = self.jobs.lock();
        if let Some(entry) = jobs.get_mut(&key) {
            entry.generation = generation;
            entry.deadline = deadline;
            entry.job = job;
            entry.notify.notify_one();
            drop(jobs);

            log::trace!(target: "app", "one-shot task `{key}` rescheduled in {delay:?}");
            return Ok(());
        }

        if jobs.len() >= MAX_PENDING_ONE_SHOT_TASKS {
            log::warn!(
                target: "app",
                "one-shot task `{key}` rejected: pending limit {} reached",
                MAX_PENDING_ONE_SHOT_TASKS
            );
            bail!(
                "pending one-shot task limit reached ({}); task `{key}` rejected",
                MAX_PENDING_ONE_SHOT_TASKS
            );
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
                job,
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

    /// 取消一次性任务；返回是否确实发出了取消。
    ///
    /// 待触发任务会立即移除并 abort。已经进入执行的可取消任务只会收到协作
    /// 取消信号，是否停止由任务自身决定；普通运行中任务无法取消。
    pub fn cancel(&self, key: &str) -> bool {
        let pending = self.jobs.lock().remove(key);
        let mut cancelled = false;

        if let Some(entry) = pending {
            entry.notify.notify_one();
            entry.handle.abort();
            cancelled = true;
        }

        let tokens: Vec<CancelToken> = self
            .running
            .lock()
            .get(key)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.token.clone())
                    .collect()
            })
            .unwrap_or_default();
        if !tokens.is_empty() {
            for token in &tokens {
                token.cancel();
            }
            cancelled = true;
        }

        if cancelled {
            log::debug!(target: "app", "one-shot task `{key}` cancel requested");
        }
        cancelled
    }

    /// 取消所有待触发任务，并向所有运行中的可取消任务发出取消信号。
    pub fn cancel_all(&self) {
        let entries: Vec<(String, OneShotEntry)> = self.jobs.lock().drain().collect();
        let running: Vec<(String, CancelToken)> = {
            let running = self.running.lock();
            let mut tokens = Vec::new();
            for (key, entries) in running.iter() {
                for entry in entries {
                    if let Some(token) = entry.token.clone() {
                        tokens.push((key.clone(), token));
                    }
                }
            }
            tokens
        };

        if entries.is_empty() && running.is_empty() {
            return;
        }

        let pending_keys: Vec<&str> = entries.iter().map(|(key, _)| key.as_str()).collect();
        for (_, entry) in &entries {
            entry.notify.notify_one();
            entry.handle.abort();
        }

        let running_keys: Vec<&str> = running.iter().map(|(key, _)| key.as_str()).collect();
        for (_, token) in &running {
            token.cancel();
        }

        log::debug!(
            target: "app",
            "one-shot tasks cancelled: {} pending keys: {pending_keys:?}, {} running keys: {running_keys:?}",
            entries.len(),
            running.len()
        );
    }

    /// 返回所有尚未触发的一次性任务快照，按 key 字典序排列。
    pub fn pending_tasks(&self) -> Vec<OneShotTaskInfo> {
        let now = tokio::time::Instant::now();
        let jobs = self.jobs.lock();
        let mut tasks: Vec<OneShotTaskInfo> = jobs
            .iter()
            .map(|(key, entry)| OneShotTaskInfo {
                key: key.clone(),
                remaining_ms: entry
                    .deadline
                    .saturating_duration_since(now)
                    .as_millis()
                    .min(u64::MAX as u128) as u64,
                generation: entry.generation,
            })
            .collect();
        tasks.sort_by(|left, right| left.key.cmp(&right.key));
        tasks
    }

    /// 返回所有尚未触发的一次性任务 key，按字典序排列。
    #[allow(dead_code)]
    pub fn pending_keys(&self) -> Vec<String> {
        let jobs = self.jobs.lock();
        let mut keys: Vec<String> = jobs.keys().cloned().collect();
        keys.sort();
        keys
    }

    /// 返回所有正在执行的一次性任务快照，按 key 字典序排列。
    pub fn running_tasks(&self) -> Vec<RunningTaskInfo> {
        let running = self.running.lock();
        let mut tasks = Vec::new();
        for (key, entries) in running.iter() {
            for entry in entries {
                tasks.push(RunningTaskInfo {
                    key: key.clone(),
                    generation: entry.generation,
                    cancellable: entry.token.is_some(),
                });
            }
        }
        tasks.sort_by(|left, right| {
            left.key
                .cmp(&right.key)
                .then_with(|| left.generation.cmp(&right.generation))
        });
        tasks
    }

    /// 指定一次性任务是否仍在等待触发。
    pub fn is_scheduled(&self, key: &str) -> bool {
        self.jobs.lock().contains_key(key)
    }

    /// 指定一次性任务是否正在执行。
    #[allow(dead_code)]
    pub fn is_running(&self, key: &str) -> bool {
        self.running.lock().contains_key(key)
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
                let (job, token, _running) = {
                    let mut jobs = scheduler.jobs.lock();
                    let Some(entry) = jobs.get(&key) else {
                        return;
                    };
                    if entry.generation != generation {
                        continue;
                    }
                    let Some(entry) = jobs.remove(&key) else {
                        return;
                    };
                    let job = entry.job;
                    let token = match &job {
                        OneShotJob::Plain(_) => None,
                        OneShotJob::Cancellable(_) => Some(CancelToken::new()),
                    };
                    let running =
                        RunningTaskGuard::new(key.clone(), generation, token.clone());
                    (job, token, running)
                };

                log::debug!(target: "app", "one-shot task `{key}` fired");
                let result = match job {
                    OneShotJob::Plain(run) => run().await,
                    OneShotJob::Cancellable(run) => {
                        let token = token.expect("cancellable one-shot task must have a token");
                        run(token).await
                    }
                };
                if let Err(err) = result {
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

struct RunningTaskGuard {
    key: String,
    generation: u64,
}

impl RunningTaskGuard {
    fn new(key: String, generation: u64, token: Option<CancelToken>) -> Self {
        OneShotScheduler::global()
            .running
            .lock()
            .entry(key.clone())
            .or_default()
            .push(RunningEntry { generation, token });
        Self { key, generation }
    }
}

impl Drop for RunningTaskGuard {
    fn drop(&mut self) {
        let mut running = OneShotScheduler::global().running.lock();
        let mut remove_key = false;
        if let Some(entries) = running.get_mut(&self.key) {
            entries.retain(|entry| entry.generation != self.generation);
            remove_key = entries.is_empty();
        }
        if remove_key {
            running.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{test_lock, JobFn};
    use super::{CancellableJobFn, OneShotScheduler, MAX_PENDING_ONE_SHOT_TASKS};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    type Events = Arc<Mutex<Vec<&'static str>>>;
    type Values = Arc<Mutex<Vec<usize>>>;

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

    fn rescheduling_value_job(events: Values, key: &'static str, remaining: usize) -> JobFn {
        Arc::new(move || {
            let events = events.clone();
            Box::pin(async move {
                events.lock().unwrap().push(remaining);
                if remaining > 1 {
                    OneShotScheduler::global().schedule_once(
                        key,
                        Duration::from_millis(20),
                        rescheduling_value_job(events, key, remaining - 1),
                    )?;
                }
                Ok(())
            })
        })
    }

    fn cancellable_wait_job(events: Events) -> CancellableJobFn {
        Arc::new(move |token| {
            let events = events.clone();
            Box::pin(async move {
                events.lock().unwrap().push("started");
                token.cancelled().await;
                events.lock().unwrap().push("cancelled");
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
    fn one_shot_can_reschedule_itself() {
        let _guard = test_lock();
        let events: Values = Arc::new(Mutex::new(Vec::new()));
        let key = "test.one_shot.reschedule_self";
        let scheduler = OneShotScheduler::global();

        scheduler
            .schedule_once(
                key,
                Duration::from_millis(20),
                rescheduling_value_job(events.clone(), key, 3),
            )
            .unwrap();

        assert!(wait_until(Duration::from_secs(2), || {
            events.lock().unwrap().len() == 3
        }));
        assert_eq!(events.lock().unwrap().as_slice(), &[3, 2, 1]);
        assert!(!scheduler.is_scheduled(key));
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
    fn cancel_requests_running_cancellable_task() {
        let _guard = test_lock();
        let events = Arc::new(Mutex::new(Vec::new()));
        let key = "test.one_shot.cancel_running";
        let scheduler = OneShotScheduler::global();
        scheduler.cancel_all();

        scheduler
            .schedule_once_cancellable(
                key,
                Duration::from_millis(20),
                cancellable_wait_job(events.clone()),
            )
            .unwrap();

        assert!(wait_until(Duration::from_secs(2), || {
            events
                .lock()
                .unwrap()
                .iter()
                .any(|value| *value == "started")
        }));
        assert!(scheduler.is_running(key));
        let running = scheduler.running_tasks();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].key, key);
        assert!(running[0].cancellable);

        assert!(scheduler.cancel(key));
        assert!(wait_until(Duration::from_secs(2), || {
            events
                .lock()
                .unwrap()
                .iter()
                .any(|value| *value == "cancelled")
        }));
        assert!(!scheduler.is_running(key));
    }

    #[test]
    fn cancel_all_requests_running_cancellable_tasks() {
        let _guard = test_lock();
        let events = Arc::new(Mutex::new(Vec::new()));
        let key = "test.one_shot.cancel_all_running";
        let scheduler = OneShotScheduler::global();
        scheduler.cancel_all();

        scheduler
            .schedule_once_cancellable(
                key,
                Duration::from_millis(20),
                cancellable_wait_job(events.clone()),
            )
            .unwrap();

        assert!(wait_until(Duration::from_secs(2), || {
            events
                .lock()
                .unwrap()
                .iter()
                .any(|value| *value == "started")
        }));
        scheduler.cancel_all();
        assert!(wait_until(Duration::from_secs(2), || {
            events
                .lock()
                .unwrap()
                .iter()
                .any(|value| *value == "cancelled")
        }));
        assert!(!scheduler.is_running(key));
    }

    #[test]
    fn pending_limit_rejects_new_keys_but_allows_updates() {
        let _guard = test_lock();
        let events = Arc::new(Mutex::new(Vec::new()));
        let scheduler = OneShotScheduler::global();
        scheduler.cancel_all();

        for index in 0..MAX_PENDING_ONE_SHOT_TASKS {
            scheduler
                .schedule_once(
                    format!("test.one_shot.limit.{index}"),
                    Duration::from_secs(3600),
                    recording_job(events.clone(), "unused"),
                )
                .unwrap();
        }
        assert_eq!(scheduler.pending_keys().len(), MAX_PENDING_ONE_SHOT_TASKS);

        let error = scheduler
            .schedule_once(
                "test.one_shot.limit.extra",
                Duration::from_secs(3600),
                recording_job(events, "unused"),
            )
            .unwrap_err();
        assert!(error.to_string().contains("limit"));

        // 已存在的 key 仍然可以更新，不受新增 key 上限影响。
        scheduler
            .schedule_once(
                "test.one_shot.limit.0",
                Duration::from_secs(3600),
                recording_job(Arc::new(Mutex::new(Vec::new())), "updated"),
            )
            .unwrap();
        assert!(scheduler
            .pending_keys()
            .contains(&"test.one_shot.limit.0".to_string()));

        scheduler.cancel_all();
        assert!(scheduler.pending_keys().is_empty());
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
}
