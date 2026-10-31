use crate::config::{
    IProfiles, PrfItem, GLOBAL_SCRIPT_DESC, GLOBAL_SCRIPT_FILE, GLOBAL_SCRIPT_NAME,
    GLOBAL_SCRIPT_UID,
};
use crate::utils::{dirs, help, tmpl};
use anyhow::{Context, Result};
use std::{
    fs,
    path::Path,
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
    let mut profiles = match help::read_yaml::<IProfiles>(&path) {
        Ok(profiles) => profiles,
        Err(error) => {
            log::warn!(target: "app", "profile catalog load failed: {error:#}");
            if path.exists() {
                preserve_invalid(&path)?;
            }
            IProfiles::template()
        }
    };
    let mut changed = normalize(&mut profiles);
    let items = profiles.items.get_or_insert_with(Vec::new);
    if let Some(item) = items
        .iter_mut()
        .find(|item| item.uid.as_deref() == Some(GLOBAL_SCRIPT_UID))
    {
        if item.itype.as_deref() != Some("script")
            || item.file.as_deref() != Some(GLOBAL_SCRIPT_FILE)
            || item.name.as_deref() != Some(GLOBAL_SCRIPT_NAME)
            || item.desc.as_deref() != Some(GLOBAL_SCRIPT_DESC)
        {
            item.itype = Some("script".into());
            item.file = Some(GLOBAL_SCRIPT_FILE.into());
            item.name = Some(GLOBAL_SCRIPT_NAME.into());
            item.desc = Some(GLOBAL_SCRIPT_DESC.into());
            changed = true;
        }
    } else {
        items.insert(0, PrfItem::global_script());
        changed = true;
    }
    let global_script = help::resolve_profile_path(GLOBAL_SCRIPT_FILE)?;
    if !global_script.exists() {
        help::write_file_atomic(&global_script, tmpl::ITEM_GLOBAL_SCRIPT.as_bytes())?;
    }
    if changed {
        help::save_yaml(&path, &profiles, Some("# Profiles Config for Clash Astra"))?;
    }
    Ok(profiles)
}

/// 迁移仅补齐历史缺失字段与历史默认名称，保留用户自定义名称。
fn normalize(profiles: &mut IProfiles) -> bool {
    let mut changed = profiles.items.is_none();
    for item in profiles.items.get_or_insert_with(Vec::new) {
        if item.uid.is_none() {
            item.uid = Some(help::get_uid("d"));
            changed = true;
        }
        let name = match (item.itype.as_deref(), item.name.as_deref()) {
            (Some("remote"), Some("remote file")) => Some("远程订阅"),
            (Some("local"), Some("local file")) => Some("本地文件"),
            (Some("script"), Some("script file")) => Some("脚本文件"),
            (Some("merge"), Some("merge file")) => Some("合并文件"),
            _ => None,
        };
        if let Some(name) = name {
            item.name = Some(name.into());
            changed = true;
        }
    }
    if let Some(chain) = &mut profiles.chain {
        let previous_len = chain.len();
        chain.retain(|uid| uid != GLOBAL_SCRIPT_UID);
        changed |= previous_len != chain.len();
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_is_idempotent_and_preserves_user_names() {
        let mut profiles = IProfiles {
            items: Some(vec![
                PrfItem {
                    itype: Some("remote".into()),
                    name: Some("remote file".into()),
                    ..Default::default()
                },
                PrfItem {
                    itype: Some("remote".into()),
                    name: Some("我的订阅".into()),
                    ..Default::default()
                },
            ]),
            chain: Some(vec![GLOBAL_SCRIPT_UID.into(), "user-script".into()]),
            ..Default::default()
        };
        assert!(normalize(&mut profiles));
        let items = profiles.items.as_ref().unwrap();
        assert_eq!(items[0].name.as_deref(), Some("远程订阅"));
        assert_eq!(items[1].name.as_deref(), Some("我的订阅"));
        assert_eq!(profiles.chain.as_ref().unwrap(), &["user-script"]);
        assert!(!normalize(&mut profiles));
    }
}
