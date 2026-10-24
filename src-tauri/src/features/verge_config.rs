use crate::config::*;
use crate::core::*;
use crate::log_err;
use anyhow::{bail, Result};

use super::profile_update::update_core_config;

/// 修改verge的订阅
/// 一般都是一个个的修改
pub async fn patch_verge(patch: IVerge) -> Result<()> {
    let tun_mode = patch.enable_tun_mode;
    let auto_launch = patch.enable_auto_launch;
    let system_proxy = patch.enable_system_proxy;
    let proxy_bypass = patch.system_proxy_bypass.clone();
    let language = patch.language.clone();
    let port = patch.verge_mixed_port;
    let common_tray_icon = patch.common_tray_icon;
    let sysproxy_tray_icon = patch.sysproxy_tray_icon;
    let tun_tray_icon = patch.tun_tray_icon;

    // Validate invariants first, then apply draft so follow-up operations
    // (e.g. run_core) can observe the target mode consistently.
    #[cfg(target_os = "windows")]
    {
        let service_mode = patch.enable_service_mode;
        if (service_mode == Some(true) || tun_mode == Some(true))
            && !crate::core::win_service::is_current_process_elevated()?
        {
            bail!("Service mode and TUN mode require Clash Verge to be run as administrator.");
        }
        let (current_tun_enabled, current_service_enabled) = {
            let verge_config = Config::verge();
            let current_verge = verge_config.latest();
            (
                current_verge.enable_tun_mode.unwrap_or(false),
                current_verge.enable_service_mode.unwrap_or(false),
            )
        };

        if current_tun_enabled && service_mode.is_some() {
            bail!("Disable TUN mode before changing service mode.");
        }

        if let Some(true) = service_mode {
            let status = crate::core::win_service::check_service().await?;
            if !status.installed {
                bail!("Service Mode requires clash-astra-service to be installed. Please install service first.");
            }
        }

        if let Some(true) = tun_mode {
            let status = crate::core::win_service::check_service().await?;
            if !status.installed {
                bail!("Tun mode on Windows requires clash-astra-service to be installed. Please install and enable Service Mode first.");
            }
            let service_enabled = service_mode.unwrap_or(current_service_enabled);
            if !service_enabled && service_mode != Some(true) {
                bail!("Tun mode on Windows requires Service Mode. Please install and enable Service Mode first.");
            }

            crate::core::win_service::ensure_service_ready()
                .await
                .map_err(|err| anyhow::anyhow!("Tun mode on Windows requires clash-astra-service. Please install and enable Service Mode first. {err}"))?;
        }
    }

    Config::verge().draft().patch_config(patch.clone());
    let mut core_restart_attempted = false;
    let mut core_config_update_attempted = false;
    let mut auto_launch_update_attempted = false;
    let mut sysproxy_update_attempted = false;
    let mut hotkey_update_attempted = false;
    let mut systray_update_attempted = false;

    let result: Result<()> = async {
        #[cfg(target_os = "windows")]
        {
            let service_mode = patch.enable_service_mode;
            if service_mode.is_some() {
                log::debug!(target: "app", "change service mode to {}", service_mode.unwrap());

                Config::generate()?;
                core_restart_attempted = true;
                CoreManager::global().run_core().await?;
                if service_mode == Some(false) {
                    log_err!(crate::core::win_service::stop_service_if_idle().await);
                }
            } else if tun_mode.is_some() {
                core_config_update_attempted = true;
                update_core_config().await?;
            }
        }

        if auto_launch.is_some() {
            auto_launch_update_attempted = true;
            sysopt::Sysopt::global().update_launch()?;
        }
        if system_proxy.is_some() || proxy_bypass.is_some() || port.is_some() {
            sysproxy_update_attempted = true;
            sysopt::Sysopt::global()
                .update_sysproxy()
                .map_err(|err| anyhow::anyhow!("failed to update system proxy: {err}"))?;
            sysopt::Sysopt::global().guard_proxy();
        }

        if patch.enable_proxy_guard.is_some() {
            sysopt::Sysopt::global().guard_proxy();
        }

        if let Some(hotkeys) = patch.hotkeys.clone() {
            hotkey_update_attempted = true;
            hotkey::Hotkey::global().update(hotkeys)?;
        }

        if language.is_some() {
            systray_update_attempted = true;
            handle::Handle::update_systray()?;
        } else if system_proxy.is_some()
            || tun_mode.is_some()
            || common_tray_icon.is_some()
            || sysproxy_tray_icon.is_some()
            || tun_tray_icon.is_some()
        {
            systray_update_attempted = true;
            handle::Handle::update_systray_part()?;
        }

        Config::verge().latest().save_file()?;
        <Result<()>>::Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            Config::verge().apply();
            Config::runtime().apply();
            Ok(())
        }
        Err(err) => {
            Config::verge().discard();
            Config::runtime().discard();

            let mut rollback_errors = Vec::new();
            if core_restart_attempted || core_config_update_attempted {
                let core_rollback = async {
                    Config::generate()?;
                    if core_restart_attempted {
                        CoreManager::global().run_core().await?;
                    } else {
                        CoreManager::global().update_config().await?;
                    }
                    Config::runtime().apply();
                    <Result<()>>::Ok(())
                }
                .await;
                if let Err(rollback_err) = core_rollback {
                    rollback_errors.push(format!(
                        "restoring the previous core configuration failed: {rollback_err}"
                    ));
                }
            }

            if sysproxy_update_attempted {
                if let Err(rollback_err) = sysopt::Sysopt::global().update_sysproxy() {
                    rollback_errors.push(format!(
                        "restoring the previous system proxy failed: {rollback_err}"
                    ));
                } else {
                    sysopt::Sysopt::global().guard_proxy();
                }
            }

            if auto_launch_update_attempted {
                if let Err(rollback_err) = sysopt::Sysopt::global().update_launch() {
                    rollback_errors.push(format!(
                        "restoring the previous auto-launch setting failed: {rollback_err}"
                    ));
                }
            }

            if hotkey_update_attempted {
                let old_hotkeys = Config::verge().data().hotkeys.clone().unwrap_or_default();
                if let Err(rollback_err) = hotkey::Hotkey::global().update(old_hotkeys) {
                    rollback_errors.push(format!(
                        "restoring the previous hotkeys failed: {rollback_err}"
                    ));
                }
            }

            if systray_update_attempted {
                if let Err(rollback_err) = handle::Handle::update_systray() {
                    rollback_errors.push(format!(
                        "refreshing the tray after rollback failed: {rollback_err}"
                    ));
                }
            }

            if !rollback_errors.is_empty() {
                let rollback_errors = rollback_errors.join("; ");
                log::error!(target: "app", "patch_verge rollback incomplete: {rollback_errors}");
                return Err(anyhow::anyhow!("{err}; {rollback_errors}"));
            }

            Err(err)
        }
    }
}
