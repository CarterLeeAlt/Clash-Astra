use serde::Serialize;

/// 后端推送给前端日志页的结构化应用日志事件。
#[derive(Debug, Clone, Serialize)]
pub struct AppLogEvent {
    pub r#type: String,
    pub payload: String,
}
