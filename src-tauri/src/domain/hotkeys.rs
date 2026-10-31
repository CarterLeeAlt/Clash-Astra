use anyhow::{ensure, Context, Result};
use std::collections::BTreeMap;

pub type HotkeyMap = BTreeMap<String, String>;

/// 先完整校验请求，再接触系统快捷键，避免半途遇到未知操作后留下部分注册。
pub fn parse(entries: &[String]) -> Result<HotkeyMap> {
    let mut map = BTreeMap::new();
    for entry in entries {
        let (action, key) = entry
            .split_once(',')
            .context("hotkey must contain action and accelerator")?;
        let (action, key) = (action.trim(), key.trim());
        ensure!(
            matches!(
                action,
                "open_or_close_dashboard"
                    | "clash_mode_rule"
                    | "clash_mode_global"
                    | "clash_mode_direct"
                    | "toggle_system_proxy"
                    | "toggle_tun_mode"
            ),
            "unsupported hotkey action: {action}"
        );
        ensure!(
            !key.is_empty() && !key.contains(','),
            "invalid hotkey accelerator"
        );
        ensure!(
            !map.contains_key(key),
            "duplicate hotkey accelerator: {key}"
        );
        map.insert(key.to_owned(), action.to_owned());
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_hotkeys_are_rejected_before_registration() {
        assert!(parse(&["toggle_tun_mode,Ctrl+T".into()]).is_ok());
        for entries in [
            vec!["unknown,Ctrl+T".into()],
            vec!["toggle_tun_mode,".into()],
            vec![
                "toggle_tun_mode,Ctrl+T".into(),
                "toggle_system_proxy,Ctrl+T".into(),
            ],
        ] {
            assert!(parse(&entries).is_err());
        }
    }
}
