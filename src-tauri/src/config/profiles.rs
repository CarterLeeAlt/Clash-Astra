use super::PrfItem;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// 订阅索引的持久化结构；业务修改规则位于 domain::profiles，文件操作位于基础设施层。
#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct IProfiles {
    pub current: Option<String>,
    pub chain: Option<Vec<String>>,
    pub items: Option<Vec<PrfItem>>,
}

impl IProfiles {
    pub fn template() -> Self {
        Self {
            items: Some(vec![PrfItem::global_script()]),
            chain: Some(vec![]),
            ..Default::default()
        }
    }
    pub fn get_items(&self) -> Option<&Vec<PrfItem>> {
        self.items.as_ref()
    }
    pub fn get_item(&self, uid: &str) -> Result<&PrfItem> {
        self.items
            .iter()
            .flatten()
            .find(|item| item.uid.as_deref() == Some(uid))
            .with_context(|| format!("profile not found: {uid}"))
    }
}
