use super::clash_api;
use super::logger::Logger;
use crate::config::*;
use crate::utils::dirs;
use anyhow::{bail, Context, Result};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{Pid, System};
use tauri::api::process::CommandChild;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::sleep;

mod process;

#[derive(Debug)]
pub struct CoreManager {
    sidecar: Arc<Mutex<Option<CommandChild>>>,

    #[cfg(target_os = "windows")]
    use_service_mode: Arc<Mutex<bool>>,

    core_operation: TokioMutex<()>,
    generation: AtomicU64,
    active_generation: AtomicU64,
    desired_running: AtomicBool,
    core_ready: AtomicBool,
    recovery_scheduled: AtomicBool,
    proxy_initialized: AtomicBool,
}

impl CoreManager {
    pub fn global() -> &'static CoreManager {
        static CORE_MANAGER: OnceCell<CoreManager> = OnceCell::new();

        CORE_MANAGER.get_or_init(|| CoreManager {
            sidecar: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "windows")]
            use_service_mode: Arc::new(Mutex::new(false)),
            core_operation: TokioMutex::new(()),
            generation: AtomicU64::new(0),
            active_generation: AtomicU64::new(0),
            desired_running: AtomicBool::new(false),
            core_ready: AtomicBool::new(false),
            recovery_scheduled: AtomicBool::new(false),
            proxy_initialized: AtomicBool::new(false),
        })
    }

    pub fn init(&self) -> Result<()> {
        let _ = dirs::clash_pid_path()
            .and_then(|path| fs::read_to_string(path).context("read previous core PID"))
            .and_then(|pid| pid.trim().parse::<u32>().context("parse previous core PID"))
            .and_then(|pid| {
                let app_dir = dirs::app_home_dir()?;
                let exe_dir = dirs::executable_dir()?;
                let expected: Vec<_> = [MIHOMO_CORE]
                    .iter()
                    .filter_map(|name| dunce::canonicalize(exe_dir.join(format!("{name}.exe"))).ok())
                    .collect();
                let mut system = System::new();
                let pid = Pid::from_u32(pid);
                system.refresh_process_specifics(
                    pid,
                    sysinfo::ProcessRefreshKind::new()
                        .with_exe(sysinfo::UpdateKind::Always)
                        .with_cmd(sysinfo::UpdateKind::Always),
                );
                if let Some(process) = system.process(pid) {
                    let owns_executable = process
                        .exe()
                        .and_then(|path| dunce::canonicalize(path).ok())
                        .map(|actual| {
                            expected.iter().any(|path| {
                                actual.to_string_lossy().eq_ignore_ascii_case(&path.to_string_lossy())
                            })
                        })
                        .unwrap_or(false);
                    let owns_config = process.cmd().windows(2).any(|args| {
                        args[0] == "-d"
                            && dunce::canonicalize(&args[1])
                                .ok()
                                .zip(dunce::canonicalize(&app_dir).ok())
                                .map(|(actual, expected)| {
                                    actual.to_string_lossy().eq_ignore_ascii_case(&expected.to_string_lossy())
                                })
                                .unwrap_or(false)
                    });
                    if owns_executable && owns_config {
                        log::info!(target: "app", "stopping previous app-owned Mihomo core, pid={pid}");
                        if !process.kill() {
                            bail!("could not stop previous Mihomo core, pid={pid}");
                        }
                        for _ in 0..40 {
                            if !system.refresh_process(pid) {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(50));
                        }
                    }
                }
                Ok(())
            });

        tauri::async_runtime::spawn(async {
            // 启动clash
            let manager = Self::global();
            match manager.run_core().await {
                Ok(()) => manager.initialize_sysproxy_after_core_ready().await,
                Err(err) => {
                    log::error!(target: "app", "initial core start failed: {err}");
                    manager.schedule_core_recovery();
                }
            }
        });

        Ok(())
    }

    /// 检查订阅是否正确
    pub fn check_config(&self, snapshot: &ConfigSnapshot) -> Result<()> {
        crate::core::mihomo::verify_bundled()?;
        let config_path = snapshot.write_runtime(ConfigType::Check)?;
        let config_path = dirs::path_to_str(&config_path)?;

        let app_dir = dirs::app_home_dir()?;
        let app_dir = dirs::path_to_str(&app_dir)?;

        let mut command = std::process::Command::new(crate::core::mihomo::binary_path()?);
        use std::os::windows::process::CommandExt;
        command
            .creation_flags(0x08000000)
            .args(["-t", "-d", app_dir, "-f", config_path]);
        let output = crate::infrastructure::process_control::output(
            &mut command,
            crate::application::budget::remaining(Duration::from_secs(10))?,
        )?;

        if !output.status.success() {
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let error = clash_api::parse_check_output(combined.clone());
            let error = match !error.is_empty() {
                true => error,
                false => combined.clone(),
            };
            Logger::global().set_log(combined);
            bail!("{error}");
        }

        Ok(())
    }

    /// 更新proxies那些
    /// 如果涉及端口和外部控制则需要重启
    pub async fn update_config(&self, snapshot: &ConfigSnapshot) -> Result<()> {
        log::debug!(target: "app", "try to update clash config");

        // 更新运行时订阅
        let path = snapshot.write_runtime(ConfigType::Run)?;
        let path = dirs::path_to_str(&path)?;

        // 发送请求 发送5次
        for i in 0..5 {
            crate::application::budget::check()?;
            match clash_api::put_configs_for(snapshot.clash.get_client_info(), path).await {
                Ok(_) => break,
                Err(err) => {
                    // An ambiguous response may have followed a successful reload.
                    if crate::application::budget::retryable(&err)
                        && clash_api::verify_runtime(snapshot).await.is_ok()
                        && clash_api::verify_structure(snapshot).await.is_ok()
                    {
                        break;
                    }
                    if !crate::application::budget::retryable(&err) {
                        return Err(err);
                    }
                    if i < 4 {
                        log::info!(target: "app", "{err}");
                    } else {
                        bail!(err);
                    }
                }
            }
            sleep(Duration::from_millis(250)).await;
        }

        Ok(())
    }
}
