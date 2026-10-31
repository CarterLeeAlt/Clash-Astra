use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreLock {
    pub contract: String,
    pub version: String,
    pub binary_sha256: String,
}

pub fn mihomo_lock() -> &'static CoreLock {
    static LOCK: once_cell::sync::Lazy<CoreLock> = once_cell::sync::Lazy::new(|| {
        serde_json::from_str(include_str!("../../mihomo.lock.json"))
            .expect("valid committed Mihomo lock")
    });
    &LOCK
}
