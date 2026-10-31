//! 新建订阅与增强项使用的内容模板。

/// 本地订阅模板。
pub const ITEM_LOCAL: &str = "# Clash Astra 本地订阅模板

proxies:

proxy-groups:

rules:
";

/// 合并增强项模板。
pub const ITEM_MERGE: &str = "# Clash Astra 合并增强模板
# 使用 Merge 规则增强订阅内容

prepend-rules:

prepend-rule-providers:

prepend-proxies:

prepend-proxy-providers:

prepend-proxy-groups:

append-rules:

append-rule-providers:

append-proxies:

append-proxy-providers:

append-proxy-groups:
";

/// 用户脚本模板。
pub const ITEM_SCRIPT: &str = "// 定义 main 函数，并返回修改后的配置对象。

function main(params) {
  return params;
}
";

/// 固定全局脚本的初始模板。
pub const ITEM_GLOBAL_SCRIPT: &str = r#"// 全局覆写脚本，在读取订阅后、用户增强链之前执行。
// 保留 main 函数并返回配置对象；profileName 为当前订阅名称。

function main(config, profileName) {
  const customRules = [
    //"DOMAIN-SUFFIX,baidu.com,DIRECT",
  ];

  config.rules = customRules.concat(config.rules || []);

  return config;
}
"#;
