use super::CmdResult;
use crate::{
    application::{configuration, diagnostics},
    config::*,
    core::{clash_api, logger},
    wrap_err,
};
use serde_yaml::Mapping;
use std::collections::{HashMap, VecDeque};

#[tauri::command(async)]
pub fn get_clash_info() -> CmdResult<ControllerSnapshot> {
    let snapshot = Config::snapshot();
    Ok(ControllerSnapshot {
        info: snapshot.clash.get_client_info(),
        runtime_revision: snapshot.runtime_revision,
        generation: crate::core::CoreManager::global().generation(),
    })
}

/// 地址相同也可能对应新内核或新订阅，传输层需同时绑定运行时代次。
#[derive(serde::Serialize)]
pub struct ControllerSnapshot {
    #[serde(flatten)]
    pub info: ClashInfo,
    pub runtime_revision: u64,
    pub generation: u64,
}

#[tauri::command(async)]
pub fn get_runtime_config() -> CmdResult<Option<Mapping>> {
    Ok(Config::runtime().config)
}

#[tauri::command]
pub async fn get_runtime_yaml() -> CmdResult<String> {
    let config = Config::runtime().config;
    tokio::task::spawn_blocking(move || -> CmdResult<String> {
        let config = config.ok_or_else(|| "failed to parse config to yaml file".to_string())?;
        serde_yaml::to_string(&config)
            .map_err(|err| format!("failed to convert config to yaml: {err}").into())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command(async)]
pub fn get_runtime_logs() -> CmdResult<HashMap<String, Vec<(String, String)>>> {
    Ok(Config::runtime().chain_logs)
}

#[tauri::command]
pub async fn patch_clash_config(payload: Mapping) -> CmdResult {
    wrap_err!(configuration::patch_clash(payload).await)
}

#[tauri::command]
pub async fn patch_ports(ports: crate::domain::network::PortSettings) -> CmdResult {
    wrap_err!(crate::application::configuration::patch_ports(ports).await)
}

#[tauri::command(async)]
pub fn get_clash_logs() -> CmdResult<VecDeque<String>> {
    Ok(logger::Logger::global().get_log())
}

#[tauri::command]
pub async fn clash_api_get_proxy_delay(
    name: String,
    url: Option<String>,
    timeout: i32,
) -> CmdResult<clash_api::DelayRes> {
    let result = clash_api::get_proxy_delay(name, url, timeout).await;

    match result {
        Ok(res) => Ok(res),
        Err(err) => Err(err.into()),
    }
}

/// 最近一轮后端自动延迟测试的低值结果（节点名 -> 延迟毫秒）
#[tauri::command(async)]
pub fn get_auto_delay_results() -> CmdResult<diagnostics::DelaySnapshot> {
    Ok(diagnostics::latest_auto_delay_results())
}
