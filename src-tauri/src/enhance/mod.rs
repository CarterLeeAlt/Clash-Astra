pub mod chain;
pub mod field;
mod merge;
pub mod script;
mod tun;

use self::chain::*;
use self::field::*;
use self::merge::*;
use self::script::*;
use self::tun::*;
use crate::config::IRuntime;
use serde_yaml::{Mapping, Value};
use std::collections::HashMap;

/// 编译所需的输入快照；订阅文件由基础设施层读取后传入。
pub struct CompileInput {
    pub clash_config: Mapping,
    pub enable_tun: bool,
    pub current: Mapping,
    pub global_script: Option<ChainItem>,
    pub chain: Vec<ChainItem>,
    pub profile_name: String,
}

/// 按固定顺序编译显式输入，返回运行时配置与脚本诊断；不读取全局配置、不写文件。
pub fn compile(input: CompileInput) -> IRuntime {
    let CompileInput {
        clash_config,
        enable_tun,
        current: mut config,
        global_script,
        chain,
        profile_name,
    } = input;

    let mut result_map = HashMap::new(); // 保存脚本日志

    // 全局脚本先执行，用户增强链按用户指定顺序继续处理。
    if let Some(item) = global_script {
        if let ChainType::Script(script) = item.data {
            let mut logs = vec![];
            match use_script(script, config.to_owned(), &profile_name) {
                Ok((res_config, res_logs)) => {
                    config = res_config;
                    logs.extend(res_logs);
                }
                Err(err) => logs.push(("exception".into(), err.to_string())),
            }
            result_map.insert(item.uid, logs);
        }
    }

    // 按顺序处理用户配置的增强链。
    chain.into_iter().for_each(|item| match item.data {
        ChainType::Merge(merge) => {
            config = use_merge(merge, config.to_owned());
        }
        ChainType::Script(script) => {
            let mut logs = vec![];

            match use_script(script, config.to_owned(), &profile_name) {
                Ok((res_config, res_logs)) => {
                    config = res_config;
                    logs.extend(res_logs);
                }
                Err(err) => logs.push(("exception".into(), err.to_string())),
            }

            result_map.insert(item.uid, logs);
        }
    });

    let source_has_tun = config.get("tun").is_some();
    let clash_tun_default = clash_config
        .get("tun")
        .and_then(|v| v.as_mapping())
        .cloned()
        .unwrap_or_default();

    // 合并默认的config（tun 由 use_tun 统一处理，避免来源混淆）
    for (key, value) in clash_config.into_iter() {
        if key.as_str() != Some("tun") {
            config.insert(key, value);
        }
    }

    let mut diagnostics = Vec::new();
    config = use_tun(
        config,
        enable_tun,
        source_has_tun,
        clash_tun_default,
        &mut diagnostics,
    );
    config = apply_ipv6_to_dns(config);
    config = use_sort(config);

    IRuntime {
        config: Some(config),
        chain_logs: result_map,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiler_preserves_script_chain_and_application_override_order() {
        let output = compile(CompileInput {
            clash_config: serde_yaml::from_str("mode: rule\nipv6: false").unwrap(),
            enable_tun: false,
            current: serde_yaml::from_str("mode: global\nmarker: source").unwrap(),
            global_script: Some(ChainItem::to_script(
                "global",
                "function main(config) { config.marker = 'global'; return config; }",
            )),
            chain: vec![ChainItem::to_script(
                "user",
                "function main(config) { config.marker += '-user'; return config; }",
            )],
            profile_name: "fixture".into(),
        });
        let config = output.config.unwrap();
        assert_eq!(config.get("marker").unwrap().as_str(), Some("global-user"));
        assert_eq!(config.get("mode").unwrap().as_str(), Some("rule"));
        assert_eq!(
            config.get("dns").unwrap().get("ipv6").unwrap().as_bool(),
            Some(false)
        );
    }
}

fn apply_ipv6_to_dns(mut config: Mapping) -> Mapping {
    let ipv6_enabled = config
        .get("ipv6")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let dns = config.get("dns").and_then(|v| v.as_mapping()).cloned();
    let mut dns = dns.unwrap_or_default();
    dns.insert("ipv6".into(), Value::Bool(ipv6_enabled));
    config.insert("dns".into(), Value::Mapping(dns));
    config
}
