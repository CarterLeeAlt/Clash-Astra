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
use tauri::api::process::CommandEvent;
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
        let _operation = crate::application::operation().await?;
        self.run_core_with(&Config::snapshot()).await?;
        crate::core::handle::Handle::refresh_clash();
        Ok(())
    }

    pub async fn run_core_with(&self, snapshot: &ConfigSnapshot) -> Result<()> {
        self.desired_running.store(true, Ordering::SeqCst);
        let result = self.run_core_inner(snapshot, None).await;
        if result.is_err() && self.desired_running.load(Ordering::SeqCst) && !self.is_core_ready() {
            Self::global().schedule_core_recovery();
        }
        result
    }

    pub async fn run_core_with_until(
        &self,
        snapshot: &ConfigSnapshot,
        deadline: tokio::time::Instant,
    ) -> Result<()> {
        // This attempt is owned by the switch. The process monitor must not
        // enqueue an additional recovery of an older committed snapshot.
        self.desired_running.store(false, Ordering::SeqCst);
        let result =
            tokio::time::timeout_at(deadline, self.run_core_inner(snapshot, Some(deadline))).await;
        match result {
            Ok(Ok(())) => {
                self.desired_running.store(true, Ordering::SeqCst);
                Ok(())
            }
            Ok(Err(error)) => {
                self.stop_core_async().await?;
                Err(error)
            }
            Err(_) => {
                self.desired_running.store(false, Ordering::SeqCst);
                self.core_ready.store(false, Ordering::SeqCst);
                self.active_generation.store(0, Ordering::SeqCst);
                // Keep the application operation gate while cleaning up a timed-out
                // start. This must not schedule an old snapshot for later recovery.
                self.stop_core_async().await?;
                bail!("automatic core recovery exceeded the shared switch deadline")
            }
        }
    }

    async fn run_core_inner(
        &self,
        snapshot: &ConfigSnapshot,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<()> {
        let _operation = self.core_operation.lock().await;
        self.start_core_locked(snapshot, deadline).await
    }

    async fn recover_core_if_needed(&self) -> Result<bool> {
        let _change = crate::application::operation().await?;
        let _operation = self.core_operation.lock().await;
        if !self.desired_running.load(Ordering::SeqCst) || self.is_core_ready() {
            return Ok(false);
        }
        self.start_core_locked(&Config::snapshot(), None).await?;
        crate::core::handle::Handle::refresh_clash();
        Ok(true)
    }

    pub(super) async fn start_core_locked(
        &self,
        snapshot: &ConfigSnapshot,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<()> {
        let check_deadline = || -> Result<()> {
            anyhow::ensure!(
                deadline.is_none_or(|deadline| tokio::time::Instant::now() < deadline),
                "core start deadline expired"
            );
            Ok(())
        };
        check_deadline()?;
        crate::core::mihomo::verify_bundled()?;
        check_deadline()?;
        let config_path = snapshot.write_runtime(ConfigType::Run)?;
        check_deadline()?;

        self.core_ready.store(false, Ordering::SeqCst);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.active_generation.store(generation, Ordering::SeqCst);
        log::info!(target: "app", "starting core with runtime config: {}", dirs::path_to_str(&config_path)?);
        self.log_tun_prerequisites(snapshot);

        let previous_sidecar = self.sidecar.lock().take();
        let killed_sidecar = match previous_sidecar {
            Some(child) => {
                let pid = child.pid();
                log::debug!(target: "app", "stop the core by sidecar");
                child.kill()?;
                let mut system = sysinfo::System::new();
                let mut exited = false;
                for _ in 0..20 {
                    if !system.refresh_process(sysinfo::Pid::from_u32(pid)) {
                        exited = true;
                        break;
                    }
                    sleep(Duration::from_millis(25)).await;
                }
                anyhow::ensure!(exited, "previous Mihomo process {pid} has not exited");
                true
            }
            None => false,
        };

        #[cfg(target_os = "windows")]
        let should_kill = if *self.use_service_mode.lock() {
            log::debug!(target: "app", "stop the core by service");
            win_service::stop_core_by_service().await?;
            true
        } else {
            killed_sidecar
        };

        // 这里得等一会儿
        if should_kill {
            sleep(Duration::from_millis(50)).await;
        }

        #[cfg(target_os = "windows")]
        {
            use crate::core::win_service;

            // 服务模式
            let enable = snapshot.clash_settings.enable_service_mode;
            let enable = enable.unwrap_or(false);

            *self.use_service_mode.lock() = enable;

            if enable {
                check_deadline()?;
                // 服务模式启动失败直接报错，避免误判为服务托管
                log::debug!(target: "app", "try to run core in service mode");
                let tun_enabled = snapshot.clash_settings.enable_tun_mode.unwrap_or(false);

                match (|| async {
                    win_service::ensure_service_ready().await?;
                    win_service::run_core_by_service(&config_path, snapshot).await
                })()
                .await
                {
                    Ok(_) => {
                        crate::application::profiles::restore_selected(snapshot).await?;
                        clash_api::verify_runtime(snapshot).await?;
                        clash_api::verify_tun_state(snapshot).await?;
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

        let clash_core = snapshot.clash_settings.clash_core.clone();
        let clash_core = clash_core.unwrap_or_else(|| MIHOMO_CORE.into());

        let config_path = dirs::path_to_str(&config_path)?;

        let args = vec!["-d", app_dir, "-f", config_path];

        let spawn_result = (|| -> Result<_> {
            check_deadline()?;
            let cmd = crate::core::mihomo::command(clash_core)?;
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

        if let Err(err) = async {
            clash_api::wait_for_core_ready(
                snapshot.clash.get_client_info(),
                Duration::from_secs(12),
            )
            .await?;
            clash_api::wait_subscription_ready(snapshot).await?;
            crate::application::profiles::restore_selected(snapshot).await?;
            clash_api::verify_runtime(snapshot).await?;
            clash_api::verify_tun_state(snapshot).await
        }
        .await
        {
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

    fn log_tun_prerequisites(&self, snapshot: &ConfigSnapshot) {
        let tun_enabled = snapshot.clash_settings.enable_tun_mode.unwrap_or(false);
        if !tun_enabled {
            return;
        }
        #[cfg(target_os = "windows")]
        {
            use crate::core::handle::Handle;
            use deelevate::{PrivilegeLevel, Token};
            let service_mode = snapshot.clash_settings.enable_service_mode.unwrap_or(false);
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

    pub fn generation(&self) -> u64 {
        self.active_generation.load(Ordering::SeqCst)
    }

    pub(super) async fn initialize_sysproxy_after_core_ready(&'static self) {
        if self.proxy_initialized.load(Ordering::SeqCst) {
            return;
        }

        let mut retry_delay = Duration::from_secs(1);
        while self.desired_running.load(Ordering::SeqCst) && self.is_core_ready() {
            let init_result = tokio::task::spawn_blocking(|| {
                crate::core::sysopt::Sysopt::global().init_sysproxy()
            })
            .await;

            match init_result {
                Ok(Ok(())) => {
                    self.proxy_initialized.store(true, Ordering::SeqCst);
                    // 核心就绪后立即补一轮自动延迟测试，不必等首个 2 分钟周期
                    tauri::async_runtime::spawn(async {
                        if let Err(err) =
                            crate::application::diagnostics::run_auto_delay_test().await
                        {
                            log::warn!(target: "app", "initial auto delay test failed: {err:#}");
                        }
                    });
                    return;
                }
                Ok(Err(err)) => {
                    log::error!(target: "app", "system proxy initialization failed; retrying: {err}");
                }
                Err(err) => {
                    log::error!(target: "app", "system proxy initialization task failed; retrying: {err}");
                }
            }
            // 初始化失败期间保留守卫调度，使后续周期仍有机会重新应用代理。
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

    pub(crate) async fn stop_core_async(&self) -> Result<()> {
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
        tauri::async_runtime::block_on(async {
            match tokio::time::timeout(Duration::from_secs(10), self.stop_core_async()).await {
                Ok(result) => result,
                Err(_) => {
                    log::error!(
                        target: "app",
                        "stop_core timed out after 10s; continue shutdown and let the supervisor clean up"
                    );
                    Ok(())
                }
            }
        })
    }
}
