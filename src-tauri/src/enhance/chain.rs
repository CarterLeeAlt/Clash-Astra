use serde_yaml::Mapping;

#[derive(Debug, Clone)]
pub struct ChainItem {
    pub uid: String,
    pub data: ChainType,
}

#[derive(Debug, Clone)]
pub enum ChainType {
    Merge(Mapping),
    Script(String),
}

#[derive(Debug, Clone)]
pub enum ChainSupport {
    Mihomo,
    MihomoAlpha,
}

impl ChainItem {
    /// 内建支持一些脚本
    pub fn builtin() -> Vec<(ChainSupport, ChainItem)> {
        // meta 的一些处理
        let meta_guard =
            ChainItem::to_script("verge_meta_guard", include_str!("./builtin/meta_guard.js"));

        // meta 1.13.2 alpn string 转 数组
        let hy_alpn =
            ChainItem::to_script("verge_hy_alpn", include_str!("./builtin/meta_hy_alpn.js"));

        // meta 的一些处理
        let meta_guard_alpha =
            ChainItem::to_script("verge_meta_guard", include_str!("./builtin/meta_guard.js"));

        // meta 1.13.2 alpn string 转 数组
        let hy_alpn_alpha =
            ChainItem::to_script("verge_hy_alpn", include_str!("./builtin/meta_hy_alpn.js"));

        vec![
            (ChainSupport::Mihomo, hy_alpn),
            (ChainSupport::Mihomo, meta_guard),
            (ChainSupport::MihomoAlpha, hy_alpn_alpha),
            (ChainSupport::MihomoAlpha, meta_guard_alpha),
        ]
    }

    pub fn to_script<U: Into<String>, D: Into<String>>(uid: U, data: D) -> Self {
        Self {
            uid: uid.into(),
            data: ChainType::Script(data.into()),
        }
    }
}

impl ChainSupport {
    pub fn is_support(&self, core: Option<&String>) -> bool {
        match core {
            Some(core) => match (self, core.as_str()) {
                (ChainSupport::Mihomo, "mihomo") => true,
                (ChainSupport::MihomoAlpha, "mihomo-alpha") => true,
                _ => false,
            },
            None => true,
        }
    }
}
