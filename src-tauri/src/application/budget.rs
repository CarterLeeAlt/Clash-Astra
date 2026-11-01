use anyhow::{Context, Result};
use std::{cell::Cell, future::Future, time::Duration};
use tokio::time::Instant;

pub const CONFIG_TIMEOUT: Duration = Duration::from_secs(30);
pub const UPDATE_TIMEOUT: Duration = Duration::from_secs(60);
const RECOVERY_RESERVE: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct DeadlineExceeded(pub &'static str);
impl std::fmt::Display for DeadlineExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for DeadlineExceeded {}

#[derive(Clone, Copy)]
pub struct Budget {
    pub deadline: Instant,
    recovery: bool,
}

tokio::task_local! { static CURRENT: Budget; }
thread_local! { static BLOCKING: Cell<Option<Budget>> = const { Cell::new(None) }; }

impl Budget {
    pub fn new(timeout: Duration) -> Self {
        Self {
            deadline: Instant::now() + timeout.min(UPDATE_TIMEOUT),
            recovery: false,
        }
    }

    pub fn current() -> Option<Self> {
        CURRENT
            .try_with(|budget| *budget)
            .ok()
            .or_else(|| BLOCKING.get())
    }

    fn step_deadline(self) -> Instant {
        if self.recovery {
            self.deadline
        } else {
            self.deadline - RECOVERY_RESERVE
        }
    }

    pub async fn scope<T>(self, future: impl Future<Output = T>) -> T {
        CURRENT.scope(self, future).await
    }

    pub async fn recover<T>(mut self, future: impl Future<Output = T>) -> T {
        self.recovery = true;
        self.scope(future).await
    }

    pub fn blocking<T>(self, work: impl FnOnce() -> T) -> T {
        let previous = BLOCKING.replace(Some(self));
        struct Reset(Option<Budget>);
        impl Drop for Reset {
            fn drop(&mut self) {
                BLOCKING.set(self.0);
            }
        }
        let _reset = Reset(previous);
        work()
    }
}

pub fn remaining(limit: Duration) -> Result<Duration> {
    let limit = limit.min(UPDATE_TIMEOUT);
    let left = Budget::current().map_or(limit, |budget| {
        budget
            .step_deadline()
            .saturating_duration_since(Instant::now())
            .min(limit)
    });
    if left.is_zero() {
        return Err(DeadlineExceeded("operation deadline expired").into());
    }
    Ok(left)
}

pub fn check() -> Result<()> {
    remaining(Duration::from_secs(1)).map(|_| ())
}

pub fn is_recovery() -> bool {
    Budget::current().is_some_and(|budget| budget.recovery)
}

pub async fn step<T>(limit: Duration, future: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(remaining(limit)?, future)
        .await
        .context("operation step timed out")?
}

pub fn retryable(error: &anyhow::Error) -> bool {
    if let Some(error) = error.downcast_ref::<reqwest::Error>() {
        return error.is_timeout()
            || error.is_connect()
            || error.is_body()
            || error.status().is_some_and(|status| {
                matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504)
            });
    }
    false
}

pub fn permanent(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<reqwest::Error>()
        .is_some_and(|_| !retryable(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn nested_steps_and_recovery_share_a_capped_deadline() {
        let budget = Budget::new(Duration::from_secs(300));
        assert!(budget.deadline - Instant::now() <= UPDATE_TIMEOUT);
        let short = Budget {
            deadline: Instant::now() + Duration::from_millis(50),
            recovery: true,
        };
        short
            .scope(async {
                tokio::time::sleep(Duration::from_millis(30)).await;
                assert!(remaining(CONFIG_TIMEOUT).unwrap() < Duration::from_millis(30));
                assert!(step(CONFIG_TIMEOUT, async {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    Ok(())
                })
                .await
                .is_err());
            })
            .await;
    }

    #[tokio::test]
    async fn recovery_is_reserved_and_expired_work_cannot_start() {
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(1),
            recovery: false,
        };
        budget
            .scope(async {
                assert!(check().is_err());
            })
            .await;
        budget
            .recover(async {
                assert!(check().is_ok());
            })
            .await;
        assert!(!retryable(&anyhow::anyhow!("invalid configuration")));
    }

    #[tokio::test]
    async fn caller_timeout_does_not_release_an_inflight_worker_gate_or_allow_late_writes() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let wrote = Arc::new(AtomicBool::new(false));
        let worker_gate = gate.clone();
        let worker_wrote = wrote.clone();
        let budget = Budget {
            deadline: Instant::now() + Duration::from_millis(50),
            recovery: true,
        };
        let result = budget
            .scope(super::super::dispatch_with_budget(
                UPDATE_TIMEOUT,
                async move {
                    let _guard = worker_gate.lock().await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    check()?;
                    worker_wrote.store(true, Ordering::SeqCst);
                    Ok(())
                },
            ))
            .await;
        assert!(result.is_err());
        assert!(gate.try_lock().is_err());
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(gate.try_lock().is_ok());
        assert!(!wrote.load(Ordering::SeqCst));
    }
}
