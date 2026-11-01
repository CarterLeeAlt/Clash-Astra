use anyhow::Result;

#[derive(Debug)]
pub struct OperationFailure {
    pub stage: &'static str,
    pub cause: String,
    pub recovery_error: Option<String>,
}

impl std::fmt::Display for OperationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "configuration operation failed during {}: {}",
            self.stage, self.cause
        )?;
        match &self.recovery_error {
            Some(error) => write!(formatter, "; recovery incomplete: {error}"),
            None => write!(formatter, "; previous state restored"),
        }
    }
}
impl std::error::Error for OperationFailure {}

/// 候选配置与补偿状态属于单次操作；读取方始终访问已提交快照。
pub trait TransactionDriver {
    async fn prepare(&mut self) -> Result<()>;
    async fn apply(&mut self) -> Result<()>;
    async fn commit(&mut self) -> Result<()>;
    async fn complete(&mut self) -> Result<()> {
        Ok(())
    }
    async fn rollback(&mut self) -> Result<()>;
    fn publish(&mut self);
}

pub async fn execute(driver: &mut impl TransactionDriver) -> Result<()> {
    let mut stage = "prepare";
    let result = Box::pin(async {
        Box::pin(driver.prepare()).await?;
        stage = "apply";
        Box::pin(driver.apply()).await?;
        stage = "commit";
        Box::pin(driver.commit()).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await;
    match result {
        Ok(()) => {
            let completion = Box::pin(driver.complete()).await;
            driver.publish();
            completion.map_err(|error| CompletionFailure(format!("{error:#}")).into())
        }
        Err(error) => {
            let rollback = if let Some(budget) = super::budget::Budget::current() {
                budget.recover(Box::pin(driver.rollback())).await
            } else {
                Box::pin(driver.rollback()).await
            };
            Err(OperationFailure {
                stage,
                cause: format!("{error:#}"),
                recovery_error: rollback.err().map(|error| format!("{error:#}")),
            }
            .into())
        }
    }
}

#[derive(Debug)]
pub struct CompletionFailure(pub String);

impl std::fmt::Display for CompletionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "configuration committed but automatic application could not be verified: {}",
            self.0
        )
    }
}
impl std::error::Error for CompletionFailure {}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        fail: &'static str,
        external: u32,
        committed: u32,
        persisted: u32,
        restored: bool,
    }
    impl TransactionDriver for Fake {
        async fn prepare(&mut self) -> Result<()> {
            anyhow::ensure!(self.fail != "prepare", "rejected");
            Ok(())
        }
        async fn apply(&mut self) -> Result<()> {
            self.external = 2;
            anyhow::ensure!(self.fail != "apply", "rejected");
            Ok(())
        }
        async fn commit(&mut self) -> Result<()> {
            self.persisted = 2;
            anyhow::ensure!(self.fail != "commit", "disk full");
            Ok(())
        }
        async fn complete(&mut self) -> Result<()> {
            anyhow::ensure!(
                self.fail != "complete",
                "automatic recovery could not be verified"
            );
            Ok(())
        }
        async fn rollback(&mut self) -> Result<()> {
            self.external = 1;
            self.persisted = 1;
            self.restored = true;
            Ok(())
        }
        fn publish(&mut self) {
            self.committed = 2;
        }
    }

    #[tokio::test]
    async fn every_failure_keeps_readers_on_the_previous_state() {
        for fail in ["prepare", "apply", "commit", ""] {
            let mut driver = Fake {
                fail,
                external: 1,
                committed: 1,
                persisted: 1,
                restored: false,
            };
            let result = execute(&mut driver).await;
            let expected = if fail.is_empty() { 2 } else { 1 };
            assert_eq!(result.is_ok(), fail.is_empty());
            assert_eq!(
                (driver.external, driver.committed, driver.persisted),
                (expected, expected, expected)
            );
            assert_eq!(driver.restored, !fail.is_empty());
        }
    }

    #[tokio::test]
    async fn incomplete_compensation_preserves_both_causes() {
        struct Failure;
        impl TransactionDriver for Failure {
            async fn prepare(&mut self) -> Result<()> {
                Ok(())
            }
            async fn apply(&mut self) -> Result<()> {
                anyhow::bail!("controller rejected")
            }
            async fn commit(&mut self) -> Result<()> {
                panic!("must not commit")
            }
            async fn rollback(&mut self) -> Result<()> {
                anyhow::bail!("disk unavailable")
            }
            fn publish(&mut self) {
                panic!("must not publish")
            }
        }
        let error = execute(&mut Failure).await.unwrap_err();
        let failure = error.downcast_ref::<OperationFailure>().unwrap();
        assert_eq!(failure.stage, "apply");
        assert!(failure.cause.contains("controller rejected"));
        assert!(failure
            .recovery_error
            .as_ref()
            .unwrap()
            .contains("disk unavailable"));
    }

    #[tokio::test]
    async fn completion_failure_publishes_committed_intent_but_returns_incomplete_application() {
        let mut driver = Fake {
            fail: "complete",
            external: 1,
            committed: 1,
            persisted: 1,
            restored: false,
        };
        let error = execute(&mut driver).await.unwrap_err();
        assert!(error.is::<CompletionFailure>());
        assert_eq!(
            (driver.external, driver.persisted, driver.committed),
            (2, 2, 2)
        );
        assert!(!driver.restored);
    }
}
