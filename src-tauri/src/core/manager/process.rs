use super::CoreManager;
use crate::config::*;
use crate::core::clash_api;
use crate::core::logger::Logger;
use crate::log_err;
use crate::utils::dirs;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::api::process::{Command, CommandEvent};
use tokio::time::sleep;

#[cfg(target_os = "windows")]
use crate::core::scheduler::{JobFn, JobFuture, OneShotScheduler};
#[cfg(target_os = "windows")]
use crate::core::win_service;
#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicU8;

impl CoreManager {
    /// 启动核心
    pub async fn run_core(&self) -> Result<()> {
        self.desired_running.store(true, Ordering::SeqCst);
        let result = self.run_core_inner().await;
        if result.is_err() && self.desired_running.load(Ordering::SeqCst) && !self.is_core_ready() {
            Self::global().schedule_core_recovery();
        }
        result
    }

    async fn run_core_inner(&self) -> Result<()> {
        let _operation = self.core_operation.lock().await;
        self.start_core_locked().await
    }

    async fn recover_core_if_needed(&self) -> Result<bool> {
        let _operation = self.core_operation.lock().await;
        if !self.desired_running.load(Ordering::SeqCst) || self.is_core_ready() {
            return Ok(false);
        }
        self.start_core_locked().await?;
        Ok(true)
    }

    pub(super) async fn start_core_locked(&self) -> Result<()> {
        let config_path = Config::generate_file(ConfigType::Run)?;

        self.core_ready.store(false, Ordering::SeqCst);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.active_generation.store(generation, Ordering::SeqCst);
        log::info!(target: "app", "starting core with runtime config: {}", dirs::path_to_str(&config_path)?);
        self.log_tun_prerequisites();

        let killed_sidecar = match self.sidecar.lock().take() {
            Some(child) => {
                log::debug!(target: "app", "stop the core by sidecar");
                let _ = child.kill();
                true
            }
            None => false,
        };

        #[cfg(target_os = "windows")]
        let should_kill = if *self.use_service_mode.lock() {
            log::debug!(target: "app", "stop the core by service");
            log_err!(win_service::stop_core_by_service().await);
            true
        } else {
            killed_sidecar
        };

        // 这里得等一会儿
        if should_kill {
            sleep(Duration::from_millis(500)).await;
        }

        #[cfg(target_os = "windows")]
        {
            use crate::core::win_service;

            // 服务模式
            let enable = { Config::verge().latest().enable_service_mode };
            let enable = enable.unwrap_or(false);

            *self.use_service_mode.lock() = enable;

            if enable {
                // 服务模式启动失败直接报错，避免误判为服务托管
                log::debug!(target: "app", "try to run core in service mode");
                let tun_enabled = Config::verge().latest().enable_tun_mode.unwrap_or(false);

                match (|| async {
                    win_service::ensure_service_ready().await?;
                    win_service::run_core_by_service(&config_path).await
                })()
                .await
                {
                    Ok(_) => {
                        self.core_ready.store(true, Ordering::SeqCst);
                        Self::global().start_service_monitor(generation);
                        return Ok(());
                    }
                    Err(err) => {
                        self.active_generation
                            .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
                            .ok();
                        self.core_ready.store(false, Ordering::SeqCst);
                        log_err!(win_service::stop_core_by_service().await);
                        log::error!(target: "app", "Service Mode failed; service could not start Mihomo core. {err}");
                        if tun_enabled {
                            bail!(
                                "Tun mode requires a working clash-astra-service on Windows: {err}"
                            );
                        }
                        bail!("Service Mode failed; service could not start Mihomo core. {err}");
                    }
                }
            }
        }

        let app_dir = dirs::app_home_dir()?;
        let app_dir = dirs::path_to_str(&app_dir)?;

        let clash_core = { Config::verge().latest().clash_core.clone() };
        let clash_core = clash_core.unwrap_or_else(|| MIHOMO_CORE.into());

        let config_path = dirs::path_to_str(&config_path)?;

        let args = vec!["-d", app_dir, "-f", config_path];

        let spawn_result = (|| -> Result<_> {
            let cmd = Command::new_sidecar(clash_core)?;
            Ok(cmd.args(args).spawn()?)
        })();
        let (mut rx, cmd_child) = match spawn_result {
            Ok(child) => child,
            Err(err) => {
                self.active_generation
                    .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
                    .ok();
                return Err(err);
            }
        };

        // 将pid写入文件中
        crate::log_err!((|| {
            let pid = cmd_child.pid();
            let path = dirs::clash_pid_path()?;
            fs::File::create(path)
                .context("failed to create the pid file")?
                .write(format!("{pid}").as_bytes())
                .context("failed to write pid to the file")?;
            <Result<()>>::Ok(())
        })());

        {
            let mut sidecar = self.sidecar.lock();
            *sidecar = Some(cmd_child);
        }

        tauri::async_runtime::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Stdout(line) => {
                        log::info!(target: "app", "[mihomo]: {line}");
                        Logger::global().set_log(line);
                    }
                    CommandEvent::Stderr(err) => {
                        log::error!(target: "app", "[mihomo]: {err}");
                        Logger::global().set_log(err);
                    }
                    CommandEvent::Error(err) => {
                        log::error!(target: "app", "[mihomo]: {err}");
                        Logger::global().set_log(err);
                    }
                    CommandEvent::Terminated(_) => {
                        log::info!(target: "app", "Mihomo core terminated");
                        CoreManager::global().handle_core_terminated(generation);
                        break;
                    }
                    _ => {}
                }
            }
        });

        if let Err(err) = clash_api::wait_for_core_ready(Duration::from_secs(12)).await {
            if self
                .active_generation
                .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                if let Some(child) = self.sidecar.lock().take() {
                    let _ = child.kill();
                }
            }
            self.core_ready.store(false, Ordering::SeqCst);
            return Err(err).context("wait for Mihomo core readiness");
        }

        if self.active_generation.load(Ordering::SeqCst) != generation {
            bail!("Mihomo core terminated while readiness was being confirmed");
        }
        self.core_ready.store(true, Ordering::SeqCst);

        Ok(())
    }

    fn log_tun_prerequisites(&self) {
        let tun_enabled = Config::verge().latest().enable_tun_mode.unwrap_or(false);
        if !tun_enabled {
            return;
        }
        #[cfg(target_os = "windows")]
        {
            use crate::core::handle::Handle;
            use deelevate::{PrivilegeLevel, Token};
            let service_mode = Config::verge()
                .latest()
                .enable_service_mode
                .unwrap_or(false);
            let privilege = Token::with_current_process()
                .ok()
                .and_then(|t| t.privilege_level().ok());
            let is_admin = matches!(privilege, Some(PrivilegeLevel::Elevated));
            if !service_mode {
                log::error!(target: "app", "Tun mode is enabled but service mode is disabled on Windows. This usually fails without admin/wintun permissions.");
                Handle::emit_log(
                    "error",
                    "[service] Tun mode is enabled but service mode is disabled on Windows.",
                );
            } else {
                log::info!(target: "app", "Tun mode enabled on Windows with service mode.");
                Handle::emit_log(
                    "info",
                    "[service] Tun mode enabled on Windows with service mode.",
                );
            }
            if !is_admin {
                log::warn!(target: "app", "Current process is not elevated. If service mode is unavailable, Tun setup may fail due to missing admin privileges/wintun route permissions.");
                Handle::emit_log("warn", "[service] Current process is not elevated. Tun setup may fail due to missing admin privileges/wintun route permissions.");
            }
            log::info!(target: "app", "Windows Tun diagnostics: ensure clash-astra-service is active, wintun driver can be loaded, and firewall allows route/DNS hijack operations.");
            Handle::emit_log("info", "[tun] Windows Tun diagnostics: ensure service active, wintun loadable, and firewall allows route/DNS hijack operations.");
        }
    }

    pub fn is_core_ready(&self) -> bool {
        self.core_ready.load(Ordering::SeqCst)
    }

    pub(super) async fn initialize_sysproxy_after_core_ready(&'static self) {
        if self.proxy_initialized.load(Ordering::SeqCst) {
            return;
        }

        let mut retry_delay = Duration::from_secs(1);
        while self.desired_running.load(Ordering::SeqCst) && self.is_core_ready() {
            match crate::core::sysopt::Sysopt::global().init_sysproxy() {
                Ok(()) => {
                    self.proxy_initialized.store(true, Ordering::SeqCst);
                    // 核心就绪后立即补一轮自动延迟与站点检测，不必等首个 2 分钟周期
                    tauri::async_runtime::spawn(async {
                        if let Err(err) = crate::feat::run_auto_delay_test().await {
                            log::warn!(target: "app", "initial auto delay test failed: {err:#}");
                        }
                        if let Err(err) = crate::feat::run_auto_site_test().await {
                            log::warn!(target: "app", "initial auto site test failed: {err:#}");
                        }
                    });
                    return;
                }
                Err(err) => {
                    log::error!(target: "app", "system proxy initialization failed; retrying: {err}");
                }
            }
            // Keep the guard chain registered even while initialization keeps
            // failing, so every guard cycle also retries applying the proxy.
            crate::core::sysopt::Sysopt::global().guard_proxy();
            sleep(retry_delay).await;
            retry_delay = (retry_delay * 2).min(Duration::from_secs(15));
        }
    }

    fn handle_core_terminated(&'static self, generation: u64) {
        if self
            .active_generation
            .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            log::debug!(target: "app", "ignore stale core termination event for generation {generation}");
            return;
        }

        self.sidecar.lock().take();
        self.core_ready.store(false, Ordering::SeqCst);
        if self.desired_running.load(Ordering::SeqCst) {
            self.schedule_core_recovery();
        }
    }

    pub(super) fn schedule_core_recovery(&'static self) {
        if self
            .recovery_scheduled
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        tauri::async_runtime::spawn(async move {
            let mut retry_delay = Duration::from_secs(2);
            while self.desired_running.load(Ordering::SeqCst) && !self.is_core_ready() {
                sleep(retry_delay).await;
                if !self.desired_running.load(Ordering::SeqCst) || self.is_core_ready() {
                    break;
                }

                match self.recover_core_if_needed().await {
                    Ok(true) => break,
                    Ok(false) => break,
                    Err(err) => {
                        log::error!(target: "app", "failed to recover Mihomo core: {err}");
                        retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
                    }
                }
            }
            self.recovery_scheduled.store(false, Ordering::SeqCst);
            if self.desired_running.load(Ordering::SeqCst) {
                if self.is_core_ready() {
                    self.initialize_sysproxy_after_core_ready().await;
                } else {
                    self.schedule_core_recovery();
                }
            }
        });
    }

    #[cfg(target_os = "windows")]
    pub(super) fn start_service_monitor(&'static self, generation: u64) {
        self.schedule_service_monitor(generation, Arc::new(AtomicU8::new(0)));
    }

    #[cfg(target_os = "windows")]
    fn schedule_service_monitor(&'static self, generation: u64, failures: Arc<AtomicU8>) {
        let key = format!("service.monitor:{generation}");
        let task_failures = failures.clone();
        let run: JobFn = Arc::new(move || {
            let failures = task_failures.clone();
            let future: JobFuture = Box::pin(async move {
                if !self.desired_running.load(Ordering::SeqCst)
                    || self.active_generation.load(Ordering::SeqCst) != generation
                    || !*self.use_service_mode.lock()
                {
                    log::debug!(
                        target: "app",
                        "service monitor stopped: generation={generation}"
                    );
                    return Ok(());
                }

                if win_service::is_service_core_running().await {
                    failures.store(0, Ordering::SeqCst);
                    log::trace!(
                        target: "app",
                        "service monitor tick ok: generation={generation}"
                    );
                    self.schedule_service_monitor(generation, failures);
                    return Ok(());
                }

                let failure_count = failures.fetch_add(1, Ordering::SeqCst).saturating_add(1);
                log::debug!(
                    target: "app",
                    "service monitor failure: generation={generation}, consecutive={failure_count}"
                );
                if failure_count < 3 {
                    self.schedule_service_monitor(generation, failures);
                    return Ok(());
                }

                if self
                    .active_generation
                    .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    log::error!(target: "app", "service-managed Mihomo core stopped; scheduling recovery");
                    self.core_ready.store(false, Ordering::SeqCst);
                    self.schedule_core_recovery();
                }
                Ok(())
            });
            future
        });

        match OneShotScheduler::global().schedule_once(key, Duration::from_secs(2), run) {
            Ok(()) => log::trace!(
                target: "app",
                "service monitor scheduled: generation={generation}, interval=2s"
            ),
            Err(err) => log::error!(
                target: "app",
                "failed to schedule service monitor: {err}"
            ),
        }
    }

    async fn stop_core_async(&self) -> Result<()> {
        let _operation = self.core_operation.lock().await;
        self.desired_running.store(false, Ordering::SeqCst);
        self.core_ready.store(false, Ordering::SeqCst);
        self.active_generation.store(0, Ordering::SeqCst);

        #[cfg(target_os = "windows")]
        if *self.use_service_mode.lock() {
            log::debug!(target: "app", "stop the core by service");
            win_service::stop_core_by_service().await?;
            return Ok(());
        }

        if let Some(child) = self.sidecar.lock().take() {
            log::debug!(target: "app", "stop the core by sidecar");
            let _ = child.kill();
        }
        Ok(())
    }

    /// 停止核心运行
    pub fn stop_core(&self) -> Result<()> {
        tauri::async_runtime::block_on(self.stop_core_async())
    }
}
