#![cfg(target_os = "windows")]

use crate::utils::dirs;
use anyhow::{bail, Context, Result};
use deelevate::{PrivilegeLevel, Token};
use runas::Command as RunasCommand;
use std::os::windows::process::CommandExt;
use std::process::Command as StdCommand;

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn set_lan_firewall_enabled(enabled: bool) -> Result<()> {
    let helper = dirs::firewall_helper_path()?;
    if !helper.is_file() {
        bail!("Windows Firewall helper was not found");
    }

    let operation = if enabled {
        "--enable-lan"
    } else {
        "--disable-lan"
    };
    let level = Token::with_current_process()?.privilege_level()?;
    let status = match level {
        PrivilegeLevel::NotPrivileged => RunasCommand::new(&helper)
            .arg(operation)
            .show(false)
            .status()
            .context("administrator approval for Windows Firewall was denied or failed")?,
        _ => StdCommand::new(&helper)
            .arg(operation)
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .context("failed to run Windows Firewall helper")?,
    };

    if !status.success() {
        bail!(
            "Windows Firewall helper failed with status {}",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

pub fn reconcile_lan_firewall_on_startup() -> Result<()> {
    use crate::config::Config;

    let allow_lan = Config::clash()
        .latest()
        .0
        .get("allow-lan")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let elevated = !matches!(
        Token::with_current_process()?.privilege_level()?,
        PrivilegeLevel::NotPrivileged
    );

    if elevated {
        set_lan_firewall_enabled(allow_lan)?;
    } else if allow_lan {
        log::warn!(target: "app", "LAN access is enabled, but Windows Firewall rules cannot be repaired without administrator approval");
    }
    Ok(())
}

/// 前端就绪后调用：局域网已开启但当前进程无法修复防火墙规则时提示用户。
pub fn notify_lan_firewall_if_needed() {
    use crate::config::Config;
    use crate::core::handle::Handle;

    let allow_lan = Config::clash()
        .latest()
        .0
        .get("allow-lan")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let elevated = Token::with_current_process()
        .ok()
        .and_then(|token| token.privilege_level().ok())
        .map(|level| !matches!(level, PrivilegeLevel::NotPrivileged))
        .unwrap_or(false);

    if allow_lan && !elevated {
        Handle::notice_message(
            "set_config::error",
            "LAN access needs administrator approval to repair firewall rules",
        );
    }
}
