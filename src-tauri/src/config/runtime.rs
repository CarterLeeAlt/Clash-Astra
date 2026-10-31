use serde::{Deserialize, Serialize};
use serde_yaml::Mapping;
use std::collections::HashMap;
#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct IRuntime {
    pub config: Option<Mapping>,
    // 记录在订阅中（包括merge和script生成的）出现过的keys
    // 这些keys不一定都生效
    pub exists_keys: Vec<String>,
    pub chain_logs: HashMap<String, Vec<(String, String)>>,
    /// 编译阶段产生的诊断；由应用层发布，编译器不直接访问窗口或事件接口。
    pub diagnostics: Vec<String>,
}

impl IRuntime {
    pub fn new() -> Self {
        Self::default()
    }
}
