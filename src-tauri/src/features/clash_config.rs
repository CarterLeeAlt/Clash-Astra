use crate::config::*;
use crate::core::*;
use crate::log_err;
use anyhow::{bail, Context, Result};
use serde_yaml::Mapping;

use super::profile_update::update_core_config;

/// 修改clash的订阅
/// 防火墙 helper（可能等待 UAC）和系统代理更新（等待本地代理、WinINet）都是阻塞调用，
/// 不能直接在 tokio 工作线程上执行。
async fn run_blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .context("blocking task panicked or was cancelled")?
}

pub async fn patch_clash(patch: Mapping) -> Result<()> {
    let has_tun_patch = patch.get("tun").is_some();
    let allow_lan_target = patch.get("allow-lan").and_then(|value| value.as_bool());
    let has_runtime_patch = patch.get("allow-lan").is_some()
        || patch.get("ipv6").is_some()
        || patch.get("log-level").is_some();
    if patch.get("allow-lan").is_some() && allow_lan_target.is_none() {
        bail!("allow-lan must be a boolean");
    }
    if has_tun_patch {
        log::info!(target: "app", "patch clash tun config requested");
        handle::Handle::emit_log("info", "[tun] patch clash tun config requested");
    }

    #[cfg(target_os = "windows")]
    let firewall_rollback = if let Some(enabled) = allow_lan_target {
        let previous = Config::clash()
            .latest()
            .0
            .get("allow-lan")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        run_blocking(move || crate::core::win_firewall::set_lan_firewall_enabled(enabled))
            .await
            .with_context(|| {
                if enabled {
                    "failed to allow Mihomo through Windows Firewall for LAN access"
                } else {
                    "failed to remove Mihomo LAN rules from Windows Firewall"
                }
            })?;
        Some(previous)
    } else {
        None
    };

    Config::clash().draft().patch_config(patch.clone());
    let mut core_restart_attempted = false;
    let mut core_config_update_attempted = false;
    let mut sysproxy_update_attempted = false;
    let mut runtime_file_written = false;

    let result: Result<()> = async {
        let mixed_port = patch.get("mixed-port");
        let socks_port = patch.get("socks-port");
        let port = patch.get("port");
        let enable_random_port = Config::verge().latest().enable_random_port.unwrap_or(false);
        if mixed_port.is_some() && !enable_random_port {
            let changed = mixed_port.unwrap()
                != Config::verge()
                    .latest()
                    .verge_mixed_port
                    .unwrap_or(Config::clash().data().get_mixed_port());
            // 检查端口占用。配置值可能是字符串（如 "7897"），也需要解析；
            // 超出 u16 范围的值直接拒绝，避免截断成另一个端口去检查
            if changed {
                let raw_port = mixed_port.unwrap();
                let parsed_port = raw_port
                    .as_u64()
                    .or_else(|| raw_port.as_str().and_then(|s| s.trim().parse().ok()));
                match parsed_port.and_then(|p| u16::try_from(p).ok()) {
                    Some(port) => {
                        if !port_scanner::local_port_available(port) {
                            Config::clash().discard();
                            bail!("port already in use");
                        }
                    }
                    None => {
                        Config::clash().discard();
                        bail!("invalid mixed-port: {raw_port:?}");
                    }
                }
            }
        };

        // 激活订阅
        if mixed_port.is_some()
            || socks_port.is_some()
            || port.is_some()
            || patch.get("secret").is_some()
            || patch.get("external-controller").is_some()
            || allow_lan_target.is_some()
        {
            Config::generate()?;
            core_restart_attempted = true;
            runtime_file_written = true;
            CoreManager::global().run_core().await?;
            handle::Handle::refresh_clash();
        }

        // 更新系统代理
        if mixed_port.is_some() {
            sysproxy_update_attempted = true;
            run_blocking(|| sysopt::Sysopt::global().update_sysproxy()).await?;
            sysopt::Sysopt::global().guard_proxy();
        }

        if patch.get("mode").is_some() {
            log_err!(handle::Handle::update_systray_part());
        }

        if has_tun_patch {
            log::info!(target: "app", "tun config changed, reload core config");
            handle::Handle::emit_log("info", "[tun] tun config changed, reload core config");
            core_config_update_attempted = true;
            update_core_config().await?;
        }

        Config::runtime().draft().patch_config(patch);

        if has_runtime_patch {
            runtime_file_written = true;
            Config::generate_file(ConfigType::Run)?;
            handle::Handle::refresh_clash();
        }

        Config::clash().latest().save_config()?;
        <Result<()>>::Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            Config::clash().apply();
            Config::runtime().apply();
            Ok(())
        }
        Err(err) => {
            Config::clash().discard();
            Config::runtime().discard();

            let mut rollback_errors = Vec::new();
            if core_restart_attempted || core_config_update_attempted || runtime_file_written {
                let runtime_rollback = async {
                    Config::generate()?;
                    if core_restart_attempted {
                        CoreManager::global().run_core().await?;
                    } else if core_config_update_attempted {
                        CoreManager::global().update_config().await?;
                    } else {
                        Config::generate_file(ConfigType::Run)?;
                    }
                    Config::runtime().apply();
                    <Result<()>>::Ok(())
                }
                .await;
                if let Err(rollback_err) = runtime_rollback {
                    rollback_errors.push(format!(
                        "restoring the previous core configuration failed: {rollback_err}"
                    ));
                }
            }

            if sysproxy_update_attempted {
                if let Err(rollback_err) =
                    run_blocking(|| sysopt::Sysopt::global().update_sysproxy()).await
                {
                    rollback_errors.push(format!(
                        "restoring the previous system proxy failed: {rollback_err}"
                    ));
                } else {
                    sysopt::Sysopt::global().guard_proxy();
                }
            }

            #[cfg(target_os = "windows")]
            if let Some(previous) = firewall_rollback {
                if let Err(rollback_err) = run_blocking(move || {
                    crate::core::win_firewall::set_lan_firewall_enabled(previous)
                })
                .await
                {
                    rollback_errors.push(format!(
                        "restoring the previous Windows Firewall LAN rules failed: {rollback_err}"
                    ));
                }
            }

            if !rollback_errors.is_empty() {
                let rollback_errors = rollback_errors.join("; ");
                log::error!(target: "app", "patch_clash rollback incomplete: {rollback_errors}");
                return Err(anyhow::anyhow!("{err}; {rollback_errors}"));
            }

            Err(err)
        }
    }
}
