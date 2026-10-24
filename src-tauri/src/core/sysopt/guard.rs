use crate::config::Config;
use crate::core::handle::Handle;
use crate::core::scheduler::{JobFn, JobFuture, OneShotScheduler};
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use sysproxy::Sysproxy;

use super::proxy::{set_system_proxy_with_retry, wait_for_local_proxy};
use super::{Sysopt, DEFAULT_BYPASS};

/// 系统代理守护任务。
const GUARD_TASK_KEY: &str = "system_proxy.guard";
const GUARD_INITIAL_DELAY_SECS: u64 = 10;

fn system_proxy_guard_enabled(enable_system_proxy: bool, enable_proxy_guard: bool) -> bool {
    enable_system_proxy && enable_proxy_guard
}

fn guard_job() -> JobFn {
    Arc::new(|| {
        let future: JobFuture = Box::pin(run_guard_cycle());
        future
    })
}

async fn run_guard_cycle() -> Result<()> {
    let (enable, guard, guard_duration, bypass) = {
        let verge = Config::verge();
        let verge = verge.latest();
        (
            verge.enable_system_proxy.unwrap_or(false),
            verge.enable_proxy_guard.unwrap_or(false),
            verge.proxy_guard_duration.unwrap_or(10),
            verge.system_proxy_bypass.clone(),
        )
    };

    log::debug!(
        target: "app",
        "system proxy guard tick: enable_system_proxy={enable}, enable_proxy_guard={guard}, duration={guard_duration}s"
    );

    if !system_proxy_guard_enabled(enable, guard) {
        log::debug!(
            target: "app",
            "system proxy guard task stopped: enable_system_proxy={enable}, enable_proxy_guard={guard}"
        );
        return Ok(());
    }

    let interval_secs = guard_duration.max(1);
    if guard_duration == 0 {
        log::warn!(
            target: "app",
            "proxy_guard_duration is 0; using 1s to avoid a busy loop"
        );
    }

    let port = {
        Config::verge()
            .latest()
            .verge_mixed_port
            .unwrap_or(Config::clash().data().get_mixed_port())
    };

    let sysproxy = Sysproxy {
        enable: true,
        host: "127.0.0.1".into(),
        port,
        bypass: bypass.unwrap_or(DEFAULT_BYPASS.into()),
    };

    if let Err(err) =
        wait_for_local_proxy(port).context("wait for local proxy before guarding system proxy")
    {
        log::error!(target: "app", "{err}");
        return reschedule_guard(interval_secs);
    }

    {
        let _operation = Sysopt::global().proxy_operation.lock();
        let still_enabled = {
            let verge = Config::verge();
            let verge = verge.latest();
            verge.enable_system_proxy.unwrap_or(false) && verge.enable_proxy_guard.unwrap_or(false)
        };
        if !still_enabled {
            log::debug!(target: "app", "system proxy guard skipped: disabled while waiting");
            return Ok(());
        }

        let guard_result = set_system_proxy_with_retry(
            &sysproxy,
            "guard system proxy",
            cfg!(target_os = "windows"),
        );

        match &guard_result {
            Ok(()) => {
                #[cfg(target_os = "windows")]
                {
                    let mut owned = Sysopt::global().owned_windows_proxy.lock();
                    if let Some(snapshot) = owned.as_mut() {
                        if let Err(err) = snapshot.refresh_app_owned_values() {
                            log::warn!(target: "app", "could not refresh Windows proxy ownership snapshot after guard: {err}");
                        } else if let Err(err) = snapshot.record_owned_recovery(&sysproxy) {
                            log::warn!(target: "app", "could not refresh persistent Windows proxy ownership after guard: {err}");
                        }
                    }
                }
                *Sysopt::global().guard_failure_notified.lock() = false;
            }
            Err(err) => {
                log::error!(target: "app", "system proxy guard failed to reapply the proxy: {err:#}");
                let mut notified = Sysopt::global().guard_failure_notified.lock();
                if !*notified {
                    *notified = true;
                    Handle::notice_message(
                        "set_config::error",
                        format!("System proxy guard failed to reapply the proxy: {err:#}"),
                    );
                }
            }
        }
    }

    reschedule_guard(interval_secs)
}

fn reschedule_guard(interval_secs: u64) -> Result<()> {
    OneShotScheduler::global().schedule_once(
        GUARD_TASK_KEY,
        Duration::from_secs(interval_secs.max(1)),
        guard_job(),
    )?;
    log::debug!(
        target: "app",
        "system proxy guard rescheduled in {}s",
        interval_secs.max(1)
    );
    Ok(())
}

impl Sysopt {
    /// 启动系统代理守护。
    ///
    /// 首次检查在 10 秒后；每次执行结束后按最新的 `proxy_guard_duration`
    /// 重新安排下一次执行。守护由一次性任务调度器驱动，任务执行期间不会
    /// 删除自己的底层周期任务，因此不会触发 `delay_timer` 的 `finish_task`
    /// 竞态错误。
    pub fn guard_proxy(&self) {
        let scheduler = OneShotScheduler::global();
        let (enable, guard) = {
            let verge = Config::verge();
            let verge = verge.latest();
            (
                verge.enable_system_proxy.unwrap_or(false),
                verge.enable_proxy_guard.unwrap_or(false),
            )
        };

        if !system_proxy_guard_enabled(enable, guard) {
            if scheduler.cancel(GUARD_TASK_KEY) {
                log::debug!(
                    target: "app",
                    "system proxy guard task cancelled: enable_system_proxy={enable}, enable_proxy_guard={guard}"
                );
            } else {
                log::trace!(
                    target: "app",
                    "system proxy guard task not needed: enable_system_proxy={enable}, enable_proxy_guard={guard}"
                );
            }
            return;
        }

        if scheduler.is_scheduled(GUARD_TASK_KEY) {
            log::trace!(target: "app", "system proxy guard task already scheduled");
            return;
        }

        match scheduler.schedule_once(
            GUARD_TASK_KEY,
            Duration::from_secs(GUARD_INITIAL_DELAY_SECS),
            guard_job(),
        ) {
            Ok(()) => log::debug!(
                target: "app",
                "system proxy guard task registered, first check in {GUARD_INITIAL_DELAY_SECS}s"
            ),
            Err(err) => log::error!(
                target: "app",
                "failed to register system proxy guard task: {err}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::system_proxy_guard_enabled;

    #[test]
    fn proxy_guard_requires_both_switches() {
        assert!(!system_proxy_guard_enabled(false, false));
        assert!(!system_proxy_guard_enabled(false, true));
        assert!(!system_proxy_guard_enabled(true, false));
        assert!(system_proxy_guard_enabled(true, true));
    }
}
