use anyhow::{bail, Result};
use serde::Deserialize;
use serde_yaml::{Mapping, Value};

/// 一次请求描述全部监听端口，避免分次提交造成内核与系统代理不一致。
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortSettings {
    pub mixed: u16,
    pub socks: u16,
    pub http: u16,
}

impl PortSettings {
    pub fn validate(self) -> Result<Self> {
        let ports = [self.mixed, self.socks, self.http];
        if ports.iter().any(|port| *port < 1000) {
            bail!("proxy ports must be between 1000 and 65535");
        }
        if self.mixed == self.socks || self.mixed == self.http || self.socks == self.http {
            bail!("proxy listener ports must be distinct");
        }
        Ok(self)
    }

    pub fn mapping(self) -> Mapping {
        Mapping::from_iter([
            (Value::from("mixed-port"), Value::from(self.mixed)),
            (Value::from("socks-port"), Value::from(self.socks)),
            (Value::from("port"), Value::from(self.http)),
        ])
    }
}

pub fn validate_patch(patch: &Mapping) -> Result<()> {
    for key in ["mixed-port", "socks-port", "port"] {
        if let Some(value) = patch.get(key) {
            let port = value.as_u64().or_else(|| value.as_str()?.parse().ok());
            if !matches!(port, Some(1000..=65535)) {
                bail!("{key} must be between 1000 and 65535");
            }
        }
    }
    for key in ["allow-lan", "ipv6"] {
        if patch
            .get(key)
            .is_some_and(|value| value.as_bool().is_none())
        {
            bail!("{key} must be a boolean");
        }
    }
    if let Some(mode) = patch.get("mode") {
        if !matches!(mode.as_str(), Some("rule" | "global" | "direct")) {
            bail!("invalid proxy mode");
        }
    }
    Ok(())
}

/// 订阅和增强结果使用当前 Mihomo 格式，不对旧模式或旧字段做转换。
pub fn validate_subscription(config: &Mapping) -> Result<()> {
    if let Some(contract) = config.get("x-sublink-contract") {
        anyhow::ensure!(
            contract.as_str() == Some(crate::config::mihomo_lock().contract.as_str()),
            "subscription uses a different generation contract"
        );
    }
    if let Some(version) = config.get("x-mihomo-version") {
        anyhow::ensure!(
            version.as_str() == Some(crate::config::mihomo_lock().version.as_str()),
            "subscription targets a different Mihomo version"
        );
    }
    if let Some(mode) = config.get("mode") {
        if !matches!(mode.as_str(), Some("rule" | "global" | "direct")) {
            bail!("subscription mode must be rule, global or direct");
        }
    }
    if let Some(proxies) = config.get("proxies").and_then(Value::as_sequence) {
        for proxy in proxies {
            if matches!(
                proxy.get("type").and_then(Value::as_str),
                Some("hysteria" | "hysteria2")
            ) {
                if let Some(alpn) = proxy.get("alpn") {
                    anyhow::ensure!(
                        alpn.as_sequence().is_some_and(|values| values
                            .iter()
                            .all(|value| value.as_str().is_some())),
                        "Hysteria alpn must be a list of strings"
                    );
                }
            }
        }
    }
    Ok(())
}

/// 只接受可用代理源的结构；provider 的实际下载结果必须在加载阶段另行确认。
pub fn validate_sources(config: &Mapping) -> Result<()> {
    if let Some(value) = config.get("proxies") {
        anyhow::ensure!(value.is_sequence(), "proxies must be a list");
    }
    if let Some(value) = config.get("proxy-providers") {
        anyhow::ensure!(value.is_mapping(), "proxy-providers must be a mapping");
    }
    let inline = config
        .get("proxies")
        .and_then(Value::as_sequence)
        .is_some_and(|nodes| {
            nodes.iter().any(|node| {
                node.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| !matches!(kind, "direct" | "reject" | "dns"))
                    && node
                        .get("server")
                        .and_then(Value::as_str)
                        .is_some_and(|server| !server.trim().is_empty())
            })
        });
    let providers = config
        .get("proxy-providers")
        .and_then(Value::as_mapping)
        .is_some_and(|providers| !providers.is_empty());
    anyhow::ensure!(
        inline || providers,
        "subscription has no proxy source; refusing an empty or direct-only subscription"
    );
    validate_subscription(config)
}

pub fn parse_remote_profile(data: &str) -> Result<Mapping> {
    let data = data.trim_start_matches('\u{feff}').trim();
    anyhow::ensure!(!data.is_empty(), "remote subscription response is empty");
    anyhow::ensure!(
        !data.starts_with('<'),
        "remote subscription returned HTML/XML instead of Mihomo YAML"
    );
    let mut value: Value = serde_yaml::from_str(data).map_err(|_| {
        anyhow::anyhow!("remote subscription is not valid Mihomo YAML; content omitted for privacy")
    })?;
    value
        .apply_merge()
        .map_err(|_| anyhow::anyhow!("invalid YAML merge in subscription"))?;
    let config = value
        .as_mapping()
        .ok_or_else(|| anyhow::anyhow!("remote subscription must be a YAML mapping"))?;
    validate_sources(config)?;
    Ok(config.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_shape_rejects_false_successes_and_preserves_native_fields() {
        for source in [
            "",
            "<html>Error</html>",
            "c3M6Ly9ub3QtcmVhbA==",
            "proxies: []",
            "proxies: null",
            "proxies: [{type: direct, name: direct}]",
            "outbounds: []",
        ] {
            assert!(parse_remote_profile(source).is_err(), "{source}");
        }
        let source = "proxies: [{name: node, type: vless, server: example.invalid, port: 443, alpn: [h2], ech-opts: {enable: false}}]";
        assert_eq!(
            parse_remote_profile(source).unwrap(),
            serde_yaml::from_str::<Mapping>(source).unwrap()
        );
        assert!(parse_remote_profile(
            "proxy-providers: {p: {type: http, url: 'https://example.invalid/sub'}}"
        )
        .is_ok());
    }

    #[test]
    fn current_modes_and_alpn_are_validated_without_conversion() {
        for source in [
            "mode: rule",
            "mode: global",
            "mode: direct",
            "proxies: [{type: hysteria, alpn: [h3]}]",
        ] {
            validate_subscription(&serde_yaml::from_str(source).unwrap()).unwrap();
        }
        for source in ["mode: script", "proxies: [{type: hysteria, alpn: h3}]"] {
            let config = serde_yaml::from_str(source).unwrap();
            let before = serde_yaml::to_string(&config).unwrap();
            assert!(validate_subscription(&config).is_err());
            assert_eq!(serde_yaml::to_string(&config).unwrap(), before);
        }
        assert!(validate_patch(&serde_yaml::from_str("mode: script").unwrap()).is_err());
    }

    #[test]
    fn listener_ports_are_bounded_and_distinct() {
        assert!(PortSettings {
            mixed: 65535,
            socks: 1000,
            http: 1001
        }
        .validate()
        .is_ok());
        assert!(PortSettings {
            mixed: 999,
            socks: 1000,
            http: 1001
        }
        .validate()
        .is_err());
        assert!(PortSettings {
            mixed: 7897,
            socks: 7897,
            http: 7899
        }
        .validate()
        .is_err());
        for raw in [
            "mixed-port: 65536",
            "socks-port: -1",
            "port: 0",
            "allow-lan: yes",
        ] {
            let patch = serde_yaml::from_str(raw).unwrap();
            assert!(validate_patch(&patch).is_err(), "{raw}");
        }
    }
}
