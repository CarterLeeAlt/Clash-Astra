//! GUI 与 Windows 服务共享的本地控制协议；各二进制按需使用协议项。
#![allow(dead_code)]

pub const SERVICE_NAME: &str = "clash-astra-service";
pub const SERVICE_DISPLAY_NAME: &str = "Clash Astra Service";
pub const SERVICE_BINARY: &str = "clash-astra-service.exe";
pub const INSTALL_HELPER: &str = "clash-astra-service-install.exe";
pub const UNINSTALL_HELPER: &str = "clash-astra-service-uninstall.exe";
pub const FIREWALL_HELPER: &str = "clash-astra-firewall-helper.exe";
pub const API_ADDR: &str = "127.0.0.1:33211";
pub const API_HEALTH: &str = "/health";
pub const API_GET_CLASH: &str = "/get_clash";
pub const API_START_CLASH: &str = "/start_clash";
pub const API_STOP_CLASH: &str = "/stop_clash";
pub const API_STOP_SERVICE: &str = "/stop_service";
pub const API_TOKEN_DIR: &str = "service-data";
pub const API_TOKEN_FILE: &str = "service-api-token";

pub fn service_url(endpoint: &str) -> String {
    format!("http://{API_ADDR}{endpoint}")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceReply<T> {
    pub code: u64,
    pub msg: String,
    pub data: Option<T>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StartClashRequest {
    pub core_type: String,
    pub bin_path: String,
    pub config_dir: String,
    pub config_file: String,
    pub log_file: String,
}
