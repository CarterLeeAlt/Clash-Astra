use crate::config::{IProfiles, GLOBAL_SCRIPT_FILE};
use crate::domain::profiles::validate;
use crate::utils::{dirs, help, tmpl};
use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

static WRITES_BLOCKED: AtomicBool = AtomicBool::new(false);

/// 原始索引无法备份时阻止事务覆盖它，保留用户修复数据的机会。
pub fn ensure_writable() -> Result<()> {
    anyhow::ensure!(!WRITES_BLOCKED.load(Ordering::SeqCst), "profiles.yaml cannot be overwritten: the invalid original could not be backed up; repair it and restart");
    Ok(())
}

fn preserve_invalid(path: &Path) -> Result<()> {
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let backup = path.with_file_name(format!(
        "profiles.yaml.invalid-{timestamp}-{}.bak",
        std::process::id()
    ));
    let copied = fs::copy(path, &backup).context("preserve invalid profiles index")?;
    anyhow::ensure!(
        copied == fs::metadata(path)?.len() && fs::metadata(&backup)?.len() == copied,
        "invalid profiles backup verification failed"
    );
    log::warn!(target: "app", "invalid profiles index preserved at {}", backup.display());
    Ok(())
}

pub fn load() -> IProfiles {
    match load_inner() {
        Ok(profiles) => profiles,
        Err(error) => {
            WRITES_BLOCKED.store(true, Ordering::SeqCst);
            log::error!(target: "app", "profile catalog unavailable: {error:#}");
            IProfiles::template()
        }
    }
}

fn load_inner() -> Result<IProfiles> {
    let path = dirs::profiles_path()?;
    let profiles = load_at(&path)?;
    // 全局脚本文件缺失属于当前数据恢复；索引身份已经通过校验，不改写用户脚本。
    let global_script = help::resolve_profile_path(GLOBAL_SCRIPT_FILE)?;
    if !global_script.exists() {
        help::write_file_atomic(&global_script, tmpl::ITEM_GLOBAL_SCRIPT.as_bytes())?;
    }
    Ok(profiles)
}

fn load_at(path: &PathBuf) -> Result<IProfiles> {
    match help::read_yaml::<IProfiles>(path).and_then(|profiles| {
        validate(&profiles)?;
        Ok(profiles)
    }) {
        Ok(profiles) => Ok(profiles),
        Err(error) => {
            log::warn!(target: "app", "profile catalog load failed: {error:#}");
            if path.exists() {
                preserve_invalid(&path)?;
            }
            let profiles = IProfiles::template();
            help::save_yaml(path, &profiles, Some("# Profiles Config for Clash Astra"))?;
            Ok(profiles)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_catalog_is_read_without_rewriting() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.tmp/current-catalog")
            .join(format!(
                "{}-{}",
                std::process::id(),
                crate::utils::help::get_uid("test")
            ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("profiles.yaml");
        let source = serde_yaml::to_string(&IProfiles::template()).unwrap();
        fs::write(&path, &source).unwrap();
        validate(&load_at(&path).unwrap()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }

    #[test]
    fn invalid_catalog_is_preserved_before_resetting_current_state() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.tmp/current-catalog")
            .join(format!(
                "{}-{}",
                std::process::id(),
                crate::utils::help::get_uid("test")
            ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("profiles.yaml");
        let source = "items:\n  - type: remote\n    name: remote file\n";
        fs::write(&path, source).unwrap();
        let profiles = load_at(&path).unwrap();
        validate(&profiles).unwrap();
        assert_eq!(profiles.items.unwrap().len(), 1);
        let backups = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "bak"))
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), source);
    }
}
