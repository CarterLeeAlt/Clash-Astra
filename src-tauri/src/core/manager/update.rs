use super::CoreManager;
use crate::config::*;
use crate::core::clash_api;
use crate::core::logger::Logger;
use anyhow::{bail, Context, Result};
use std::fs;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::time::sleep;

#[cfg(target_os = "windows")]
use sysinfo::System;

fn core_update_channel(core: &str) -> Result<&'static str> {
    match core {
        MIHOMO_CORE => Ok("release"),
        MIHOMO_ALPHA_CORE => Ok("alpha"),
        _ => bail!("invalid Mihomo core name \"{core}\""),
    }
}

#[cfg(target_os = "windows")]
fn core_executable_path(core: &str) -> Result<std::path::PathBuf> {
    use tauri::utils::platform::current_exe;

    let app_exe = dunce::canonicalize(current_exe()?)?;
    let app_dir = app_exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("failed to get the executable directory"))?;
    let core_path = app_dir.join(format!("{core}.exe"));
    if !core_path.is_file() {
        bail!("Mihomo core executable not found: {}", core_path.display());
    }
    Ok(core_path)
}

#[cfg(target_os = "windows")]
fn core_backup_path(core_path: &std::path::Path) -> Result<std::path::PathBuf> {
    let app_dir = core_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Mihomo core executable has no parent directory"))?;
    let file_name = core_path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Mihomo core executable has no file name"))?;
    Ok(app_dir.join("meta-backup").join(file_name))
}

#[cfg(target_os = "windows")]
fn remove_file_if_exists(path: &std::path::Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(target_os = "windows")]
fn remove_empty_parent(path: &std::path::Path) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::read_dir(parent)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_none()
    {
        let _ = fs::remove_dir(parent);
    }
}

#[cfg(target_os = "windows")]
fn kill_processes_at_path(core_path: &std::path::Path) -> usize {
    let expected = core_path.to_string_lossy();
    let mut system = System::new_all();
    system.refresh_all();
    let mut killed = 0;

    for process in system.processes().values() {
        let matches = process
            .exe()
            .map(|path| path.to_string_lossy().eq_ignore_ascii_case(&expected))
            .unwrap_or(false);
        if matches && process.kill() {
            killed += 1;
        }
    }

    killed
}

impl CoreManager {
    #[cfg(target_os = "windows")]
    fn restore_running_state_after_unchanged_update(
        &'static self,
        desired_running: bool,
        core_ready: bool,
        service_mode: bool,
        generation: u64,
    ) {
        let still_active =
            generation != 0 && self.active_generation.load(Ordering::SeqCst) == generation;
        let core_ready = core_ready && still_active;
        self.desired_running
            .store(desired_running, Ordering::SeqCst);
        self.core_ready.store(core_ready, Ordering::SeqCst);
        if desired_running && core_ready && service_mode {
            self.start_service_monitor(generation);
        } else if desired_running && !core_ready {
            self.schedule_core_recovery();
        }
    }

    #[cfg(target_os = "windows")]
    async fn stop_core_after_self_update_locked(
        &self,
        core_path: &std::path::Path,
        service_mode: bool,
    ) {
        self.active_generation.store(0, Ordering::SeqCst);
        self.core_ready.store(false, Ordering::SeqCst);

        if service_mode {
            if let Err(err) = crate::core::win_service::stop_core_by_service().await {
                log::warn!(target: "app", "failed to stop service-managed core after update: {err}");
            }
        }
        if let Some(child) = self.sidecar.lock().take() {
            let _ = child.kill();
        }

        // Mihomo's /upgrade endpoint starts the replacement executable itself.
        // Reap that unmanaged process before starting it again through CoreManager.
        for _ in 0..8 {
            let killed = kill_processes_at_path(core_path);
            if killed > 0 {
                log::debug!(target: "app", "stopped {killed} self-restarted Mihomo process(es) after update");
            }
            sleep(Duration::from_millis(250)).await;
        }
    }

    #[cfg(target_os = "windows")]
    async fn activate_updated_core_locked(&self, should_run: bool) -> Result<()> {
        self.check_config()
            .context("updated Mihomo executable failed configuration validation")?;
        self.desired_running.store(should_run, Ordering::SeqCst);
        if should_run {
            self.start_core_locked()
                .await
                .context("updated Mihomo executable failed to start")?;
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    async fn rollback_core_update_locked(
        &self,
        core_path: &std::path::Path,
        backup_path: &std::path::Path,
        should_run: bool,
        service_mode: bool,
    ) -> Result<()> {
        self.desired_running.store(false, Ordering::SeqCst);
        self.stop_core_after_self_update_locked(core_path, service_mode)
            .await;

        if !backup_path.is_file() {
            bail!("Mihomo updater backup not found: {}", backup_path.display());
        }

        let failed_path = core_path.with_extension("exe.update-failed");
        remove_file_if_exists(&failed_path)?;
        if core_path.exists() {
            fs::rename(core_path, &failed_path).with_context(|| {
                format!(
                    "failed to preserve rejected Mihomo executable at {}",
                    failed_path.display()
                )
            })?;
        }
        if let Err(err) = fs::rename(backup_path, core_path) {
            if failed_path.exists() {
                let _ = fs::rename(&failed_path, core_path);
            }
            return Err(err).context("failed to restore the previous Mihomo executable");
        }

        self.activate_updated_core_locked(should_run)
            .await
            .context("restored Mihomo executable failed validation or restart")?;
        remove_file_if_exists(&failed_path)?;
        remove_empty_parent(backup_path);
        Ok(())
    }

    /// Update the selected portable Mihomo executable while retaining process
    /// ownership in CoreManager and rolling back if validation or restart fails.
    pub async fn upgrade_core(&'static self) -> Result<bool> {
        #[cfg(target_os = "windows")]
        {
            let _operation = self.core_operation.lock().await;
            if !self.is_core_ready() {
                bail!("Mihomo core is not ready");
            }

            let core = Config::verge()
                .latest()
                .clash_core
                .clone()
                .unwrap_or_else(|| MIHOMO_CORE.into());
            let channel = core_update_channel(&core)?;
            let core_path = core_executable_path(&core)?;
            let backup_path = core_backup_path(&core_path)?;
            remove_file_if_exists(&backup_path).with_context(|| {
                format!(
                    "failed to remove stale Mihomo updater backup {}",
                    backup_path.display()
                )
            })?;
            remove_empty_parent(&backup_path);

            let desired_running = self.desired_running.swap(false, Ordering::SeqCst);
            let core_ready = self.core_ready.load(Ordering::SeqCst);
            let generation = self.active_generation.load(Ordering::SeqCst);
            let service_mode = *self.use_service_mode.lock();

            let upgrade_status = clash_api::upgrade_core(channel).await;
            match upgrade_status {
                Ok(clash_api::CoreUpgradeStatus::AlreadyCurrent) => {
                    self.restore_running_state_after_unchanged_update(
                        desired_running,
                        core_ready,
                        service_mode,
                        generation,
                    );
                    return Ok(false);
                }
                Err(err) => {
                    if backup_path.is_file() {
                        let rollback = self
                            .rollback_core_update_locked(
                                &core_path,
                                &backup_path,
                                desired_running,
                                service_mode,
                            )
                            .await;
                        return match rollback {
                            Ok(()) => Err(anyhow::anyhow!(
                                "Mihomo updater failed after replacing files; the previous executable was restored: {err}"
                            )),
                            Err(rollback_err) => Err(anyhow::anyhow!(
                                "Mihomo updater failed after replacing files: {err}; rollback also failed: {rollback_err}"
                            )),
                        };
                    }
                    self.restore_running_state_after_unchanged_update(
                        desired_running,
                        core_ready,
                        service_mode,
                        generation,
                    );
                    return Err(err);
                }
                Ok(clash_api::CoreUpgradeStatus::Updated) => {}
            }

            self.stop_core_after_self_update_locked(&core_path, service_mode)
                .await;
            let activation = self.activate_updated_core_locked(desired_running).await;
            if let Err(activation_err) = activation {
                let rollback = self
                    .rollback_core_update_locked(
                        &core_path,
                        &backup_path,
                        desired_running,
                        service_mode,
                    )
                    .await;
                return match rollback {
                    Ok(()) => Err(anyhow::anyhow!(
                        "Mihomo core update failed and the previous executable was restored: {activation_err}"
                    )),
                    Err(rollback_err) => Err(anyhow::anyhow!(
                        "Mihomo core update failed: {activation_err}; rollback also failed: {rollback_err}"
                    )),
                };
            }

            match remove_file_if_exists(&backup_path) {
                Ok(()) => remove_empty_parent(&backup_path),
                Err(err) => log::warn!(
                    target: "app",
                    "updated Mihomo successfully but failed to remove backup {}: {err}",
                    backup_path.display()
                ),
            }
            Ok(true)
        }
    }

    /// 切换核心
    pub async fn change_core(&self, clash_core: Option<String>) -> Result<()> {
        let clash_core = clash_core.ok_or(anyhow::anyhow!("Mihomo core is null"))?;
        if !VALID_MIHOMO_CORES.contains(&clash_core.as_str()) {
            bail!("invalid Mihomo core name \"{clash_core}\"");
        }

        log::debug!(target: "app", "change core to `{clash_core}`");

        Config::verge().draft().clash_core = Some(clash_core);
        let mut restart_attempted = false;
        let change_result = async {
            // 更新订阅
            Config::generate()?;
            self.check_config()?;

            // 清掉旧日志
            Logger::global().clear_log();
            restart_attempted = true;
            self.run_core().await?;
            Config::save_verge_snapshot()?;
            <Result<()>>::Ok(())
        }
        .await;

        match change_result {
            Ok(_) => {
                Config::verge().apply();
                Config::runtime().apply();
                Ok(())
            }
            Err(err) => {
                Config::verge().discard();
                Config::runtime().discard();

                if restart_attempted {
                    let rollback = async {
                        Config::generate()?;
                        self.run_core().await?;
                        Config::runtime().apply();
                        <Result<()>>::Ok(())
                    }
                    .await;
                    if let Err(rollback_err) = rollback {
                        return Err(anyhow::anyhow!(
                            "{err}; restoring the previous core also failed: {rollback_err}"
                        ));
                    }
                }
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::core_update_channel;
    use crate::config::{MIHOMO_ALPHA_CORE, MIHOMO_CORE};

    #[test]
    fn core_update_channel_matches_selected_core() {
        assert_eq!(core_update_channel(MIHOMO_CORE).unwrap(), "release");
        assert_eq!(core_update_channel(MIHOMO_ALPHA_CORE).unwrap(), "alpha");
        assert!(core_update_channel("unsupported").is_err());
    }
}
