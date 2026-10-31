use crate::config::{IProfiles, PrfItem, GLOBAL_SCRIPT_UID};
use anyhow::{bail, Result};

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
