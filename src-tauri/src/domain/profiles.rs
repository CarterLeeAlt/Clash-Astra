use crate::config::{
    IProfiles, PrfItem, GLOBAL_SCRIPT_DESC, GLOBAL_SCRIPT_FILE, GLOBAL_SCRIPT_NAME,
    GLOBAL_SCRIPT_UID,
};
use anyhow::{bail, Result};
use std::collections::HashSet;

/// 当前索引必须完整且引用有效；不补字段、不改名称、不修复旧结构。
pub fn validate(profiles: &IProfiles) -> Result<()> {
    let items = profiles
        .items
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("profile items are missing"))?;
    let mut uids = HashSet::new();
    for item in items {
        let uid = item
            .uid
            .as_deref()
            .filter(|uid| !uid.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("profile UID is missing"))?;
        anyhow::ensure!(uids.insert(uid), "duplicate profile UID: {uid}");
        anyhow::ensure!(
            matches!(
                item.itype.as_deref(),
                Some("remote" | "local" | "script" | "merge")
            ),
            "invalid profile type: {uid}"
        );
        anyhow::ensure!(
            item.file
                .as_deref()
                .is_some_and(|file| !file.trim().is_empty()),
            "profile file name is missing: {uid}"
        );
    }
    let global = profiles.get_item(GLOBAL_SCRIPT_UID)?;
    anyhow::ensure!(
        items.first().and_then(|item| item.uid.as_deref()) == Some(GLOBAL_SCRIPT_UID)
            && global.itype.as_deref() == Some("script")
            && global.file.as_deref() == Some(GLOBAL_SCRIPT_FILE)
            && global.name.as_deref() == Some(GLOBAL_SCRIPT_NAME)
            && global.desc.as_deref() == Some(GLOBAL_SCRIPT_DESC),
        "invalid Global Script identity"
    );
    if let Some(uid) = &profiles.current {
        anyhow::ensure!(
            is_activatable(profiles.get_item(uid)?),
            "invalid current subscription"
        );
    }
    let mut seen = HashSet::new();
    for uid in profiles
        .chain
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("profile chain is missing"))?
    {
        anyhow::ensure!(
            uid != GLOBAL_SCRIPT_UID && seen.insert(uid),
            "invalid or duplicate enhancement chain UID: {uid}"
        );
        anyhow::ensure!(
            matches!(
                profiles.get_item(uid)?.itype.as_deref(),
                Some("merge" | "script")
            ),
            "invalid enhancement chain item: {uid}"
        );
    }
    Ok(())
}

/// 激活请求只接受业务可写字段，订阅列表由独立的增删改操作维护。
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationRequest {
    pub current: Option<String>,
    pub chain: Option<Vec<String>>,
}

pub fn is_activatable(item: &PrfItem) -> bool {
    matches!(item.itype.as_deref(), Some("remote" | "local"))
        && item.uid.as_deref() != Some(GLOBAL_SCRIPT_UID)
}

pub fn select(profiles: &mut IProfiles, uid: &str) -> Result<()> {
    let item = profiles.get_item(uid)?;
    if !is_activatable(item) {
        bail!("only remote and local profiles can be activated");
    }
    profiles.current = Some(uid.to_owned());
    Ok(())
}

pub fn reorder(profiles: &mut IProfiles, active: &str, over: &str) -> Result<()> {
    let items = profiles
        .items
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("profile list is empty"))?;
    let find = |uid: &str| {
        items
            .iter()
            .position(|item| item.uid.as_deref() == Some(uid))
    };
    let (Some(from), Some(to)) = (find(active), find(over)) else {
        bail!("profile order is stale; refresh the list and retry");
    };
    if active == GLOBAL_SCRIPT_UID || over == GLOBAL_SCRIPT_UID {
        bail!("Global Script position is fixed");
    }
    let item = items.remove(from);
    items.insert(to, item);
    Ok(())
}

pub fn remove(profiles: &mut IProfiles, uid: &str) -> Result<PrfItem> {
    if uid == GLOBAL_SCRIPT_UID {
        bail!("Global Script cannot be deleted");
    }
    let items = profiles
        .items
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("profile list is empty"))?;
    let index = items
        .iter()
        .position(|item| item.uid.as_deref() == Some(uid))
        .ok_or_else(|| anyhow::anyhow!("profile no longer exists"))?;
    let removed = items.remove(index);
    if profiles.current.as_deref() == Some(uid) {
        profiles.current = items
            .iter()
            .find(|item| is_activatable(item))
            .and_then(|item| item.uid.clone());
    }
    if let Some(chain) = profiles.chain.as_mut() {
        chain.retain(|item| item != uid);
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_template_round_trips_as_a_valid_current_catalog() {
        let profiles = IProfiles::template();
        validate(&profiles).unwrap();
        let loaded = serde_yaml::from_str(&serde_yaml::to_string(&profiles).unwrap()).unwrap();
        validate(&loaded).unwrap();
        assert_eq!(profiles.items.as_ref().unwrap().len(), 1);
        assert!(profiles.current.is_none());
    }

    #[test]
    fn incomplete_catalog_is_rejected_without_migration() {
        for profiles in [
            IProfiles::default(),
            IProfiles {
                items: Some(vec![]),
                ..IProfiles::template()
            },
            IProfiles {
                items: Some(vec![
                    PrfItem::global_script(),
                    PrfItem {
                        itype: Some("remote".into()),
                        name: Some("remote file".into()),
                        ..Default::default()
                    },
                ]),
                ..IProfiles::template()
            },
            IProfiles {
                chain: Some(vec![GLOBAL_SCRIPT_UID.into()]),
                ..IProfiles::template()
            },
            IProfiles {
                current: Some("missing".into()),
                ..IProfiles::template()
            },
        ] {
            let before = serde_json::to_value(&profiles).unwrap();
            assert!(validate(&profiles).is_err());
            assert_eq!(serde_json::to_value(&profiles).unwrap(), before);
        }
    }

    #[test]
    fn current_catalog_preserves_user_names_and_rejects_duplicate_ids() {
        let mut profiles = IProfiles::template();
        let item = PrfItem {
            uid: Some("one".into()),
            itype: Some("local".into()),
            file: Some("one.yaml".into()),
            name: Some("local file".into()),
            ..Default::default()
        };
        profiles.items.as_mut().unwrap().push(item.clone());
        validate(&profiles).unwrap();
        assert_eq!(
            profiles.get_item("one").unwrap().name.as_deref(),
            Some("local file")
        );
        profiles.items.as_mut().unwrap().push(item);
        assert!(validate(&profiles).is_err());
    }

    fn fixture() -> IProfiles {
        IProfiles {
            current: Some("one".into()),
            items: Some(vec![
                PrfItem::global_script(),
                PrfItem {
                    uid: Some("one".into()),
                    itype: Some("remote".into()),
                    ..PrfItem::default()
                },
                PrfItem {
                    uid: Some("two".into()),
                    itype: Some("local".into()),
                    ..PrfItem::default()
                },
            ]),
            chain: Some(vec!["one".into()]),
        }
    }

    #[test]
    fn removing_current_selects_a_subscription_then_empty_state() {
        let mut profiles = fixture();
        remove(&mut profiles, "one").unwrap();
        assert_eq!(profiles.current.as_deref(), Some("two"));
        assert!(profiles.chain.as_ref().unwrap().is_empty());
        remove(&mut profiles, "two").unwrap();
        assert!(profiles.current.is_none());
        assert_eq!(profiles.items.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn stale_order_does_not_mutate_the_list() {
        let mut profiles = fixture();
        let before = serde_json::to_value(&profiles).unwrap();
        assert!(reorder(&mut profiles, "missing", "two").is_err());
        assert_eq!(serde_json::to_value(&profiles).unwrap(), before);
        assert!(select(&mut profiles, GLOBAL_SCRIPT_UID).is_err());
    }
}
