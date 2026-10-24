use super::state::{
    current_time_millis, effective_reliable_mode, should_force_reliable_fallback, WindowBuildMode,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowStyleConfig {
    pub platform: String,
    pub native_decorations: bool,
    pub reliable_mode: bool,
    pub custom_frameless: bool,
}

pub fn get_window_style_config() -> WindowStyleConfig {
    let now = current_time_millis();
    let build_mode = if should_force_reliable_fallback(now) {
        WindowBuildMode::ForcedReliableFallback
    } else {
        WindowBuildMode::NormalConfigured
    };
    let reliable_mode = effective_reliable_mode(build_mode);
    let custom_frameless = cfg!(target_os = "windows") && !reliable_mode;

    WindowStyleConfig {
        platform: std::env::consts::OS.to_string(),
        native_decorations: cfg!(target_os = "windows") && reliable_mode,
        reliable_mode,
        custom_frameless,
    }
}
