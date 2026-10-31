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
        if !matches!(mode.as_str(), Some("rule" | "global" | "direct" | "script")) {
            bail!("invalid proxy mode");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
