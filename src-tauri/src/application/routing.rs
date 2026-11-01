use anyhow::{Context, Result};
use std::future::Future;
use std::time::Duration;
use tokio::time::Instant;

pub const SWITCH_TIMEOUT: Duration = Duration::from_secs(7);
const RECOVERY_RESERVE: Duration = Duration::from_secs(3);

/// A switch owns one deadline, including time spent waiting for the operation gate.
#[derive(Clone, Copy, Debug)]
pub struct SwitchBudget {
    pub deadline: Instant,
}

impl SwitchBudget {
    pub fn new() -> Self {
        Self {
            deadline: Instant::now() + SWITCH_TIMEOUT,
        }
    }

    pub fn with_remaining(milliseconds: Option<u64>) -> Self {
        let mut budget = Self::new();
        if let Some(milliseconds) = milliseconds {
            budget.deadline =
                Instant::now() + Duration::from_millis(milliseconds).min(SWITCH_TIMEOUT);
        }
        budget
    }

    /// Carry the click's absolute deadline across IPC; command dispatch latency
    /// must not grant another seven seconds when Rust finally receives it.
    pub fn from_deadline_unix_ms(deadline: Option<u64>) -> Self {
        let remaining = deadline.map(|deadline| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            deadline.saturating_sub(now)
        });
        Self::with_remaining(remaining)
    }

    pub fn recovery_at(self) -> Instant {
        (self.deadline - RECOVERY_RESERVE).max(Instant::now())
    }

    pub fn check(self) -> Result<()> {
        anyhow::ensure!(
            Instant::now() < self.deadline,
            "switch exceeded the shared 7-second deadline"
        );
        Ok(())
    }

    pub async fn run<T>(self, future: impl Future<Output = Result<T>>) -> Result<T> {
        self.check()?;
        tokio::time::timeout_at(self.deadline, future)
            .await
            .context("switch exceeded the shared 7-second deadline")?
    }
}

pub fn retry_delay(attempt: usize) -> Duration {
    Duration::from_millis([50, 100, 150, 200][attempt.min(3)])
}

pub async fn retry_until<T, F, Fut>(deadline: Instant, mut action: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0;
    loop {
        anyhow::ensure!(Instant::now() < deadline, "operation retry budget expired");
        match tokio::time::timeout_at(
            deadline.min(Instant::now() + Duration::from_millis(600)),
            action(),
        )
        .await
        {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(error)) if super::budget::permanent(&error) => return Err(error),
            result => {
                let wait = retry_delay(attempt);
                if Instant::now() + wait >= deadline {
                    return match result {
                        Ok(Err(error)) => Err(error),
                        _ => Err(anyhow::anyhow!(
                            "controller did not respond before operation deadline"
                        )),
                    };
                }
                tokio::time::sleep(wait).await;
                attempt += 1;
            }
        }
    }
}

pub async fn cleanup_or_recover(
    cleanup: &crate::core::clash_api::ConnectionCleanup,
    info: crate::config::ClashInfo,
    deadline: Instant,
    recover: impl Future<Output = Result<()>>,
) -> Result<()> {
    if let Err(error) = cleanup.finish(info, deadline).await {
        if super::budget::permanent(&error) {
            return Err(error);
        }
        recover.await?;
    }
    Ok(())
}

pub async fn close_connections(id: Option<String>) -> Result<()> {
    super::dispatch(async move {
        let _operation = super::operation().await?;
        let info = crate::config::Config::snapshot().clash.get_client_info();
        let deadline = Instant::now() + super::budget::remaining(Duration::from_secs(7))?;
        let mut before = retry_until(deadline, || {
            crate::core::clash_api::get_connections_for(info.clone())
        })
        .await?;
        if let Some(id) = id {
            before.connections.retain(|connection| connection.id == id);
        }
        crate::core::clash_api::ConnectionCleanup::capture(&before, None)
            .finish(info, deadline)
            .await
    })
    .await
}

/// Do not cancel an accepted file transaction: its worker retains the operation gate
/// until consistency cleanup finishes, but may not start another mutation after expiry.
pub async fn dispatch_switch<T: Send + 'static>(
    budget: SwitchBudget,
    work: impl Future<Output = Result<T>> + Send + 'static,
) -> Result<T> {
    let worker = tokio::spawn(Box::pin(work));
    budget
        .run(async { worker.await.context("switch worker failed")? })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_wait_is_capped_at_two_hundred_milliseconds() {
        assert_eq!(
            (0..6)
                .map(|i| retry_delay(i).as_millis())
                .collect::<Vec<_>>(),
            [50, 100, 150, 200, 200, 200]
        );
        let budget = SwitchBudget::new();
        assert_eq!(budget.deadline - budget.recovery_at(), RECOVERY_RESERVE);
    }

    #[tokio::test]
    async fn expired_operations_cannot_start_another_step() {
        assert!(SwitchBudget::from_deadline_unix_ms(Some(0))
            .check()
            .is_err());
        let budget = SwitchBudget {
            deadline: Instant::now(),
        };
        assert!(budget
            .run(async {
                panic!("expired future must not be polled");
                #[allow(unreachable_code)]
                Ok::<_, anyhow::Error>(())
            })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn queue_wait_and_api_retries_consume_the_same_deadline() {
        let budget = SwitchBudget::with_remaining(Some(100));
        tokio::time::sleep(Duration::from_millis(70)).await;
        let started = Instant::now();
        let result = budget
            .run(async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                Ok(())
            })
            .await;
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[tokio::test]
    async fn timed_out_worker_keeps_the_gate_until_safe_cleanup_but_cannot_mutate_late() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let budget = SwitchBudget::with_remaining(Some(30));
        let wrote = Arc::new(AtomicBool::new(false));
        let worker_wrote = wrote.clone();
        let result = dispatch_switch(budget, async move {
            let _operation = super::super::operation().await?;
            tokio::time::sleep(Duration::from_millis(70)).await;
            budget.check()?;
            worker_wrote.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;
        assert!(result.is_err());
        assert!(super::super::background_operation().is_none());
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(!wrote.load(Ordering::SeqCst));
        assert!(super::super::background_operation().is_some());
    }
}
