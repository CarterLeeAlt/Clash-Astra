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
pub async fn get_runtime_config() -> CmdResult<Option<Mapping>> {
    let snapshot = Config::snapshot();
    let Some(mut runtime) = snapshot.runtime.config.clone() else {
        return Ok(None);
    };
    // Persistent intent is not a readback, especially after APPLY_INCOMPLETE.
    let actual = clash_api::get_configs_for(snapshot.clash.get_client_info())
        .await
        .map_err(super::CommandError::from)?;
    let mut actual: Mapping = serde_yaml::to_value(actual)
        .map_err(anyhow::Error::from)
        .map_err(super::CommandError::from)?
        .as_mapping()
        .cloned()
        .unwrap_or_default();
    let actual_tun = actual.remove("tun");
    runtime.extend(actual);
    if let Some(tun) = actual_tun {
        if tun.get("enable").and_then(serde_yaml::Value::as_bool) == Some(true) {
            runtime.insert("tun".into(), tun);
        } else {
            // An inactive adapter has no stack/MTU to report. Keep its editor
            // preferences, but expose the actual disabled state explicitly.
            let mut preferences = runtime
                .get("tun")
                .and_then(serde_yaml::Value::as_mapping)
                .cloned()
                .unwrap_or_default();
            preferences.insert("enable".into(), false.into());
            runtime.insert("tun".into(), preferences.into());
        }
    }
    Ok(Some(runtime))
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
pub async fn patch_clash_config(payload: Mapping, deadline_unix_ms: Option<u64>) -> CmdResult {
    wrap_err!(configuration::patch_clash_until(payload, deadline_unix_ms).await)
}

#[tauri::command]
pub async fn patch_ports(ports: crate::domain::network::PortSettings) -> CmdResult {
    wrap_err!(crate::application::configuration::patch_ports(ports).await)
}

#[tauri::command]
pub async fn refresh_provider(kind: String, name: String) -> CmdResult {
    wrap_err!(configuration::refresh_network_data(Some((kind, name))).await)
}

#[tauri::command]
pub async fn refresh_geo_data() -> CmdResult {
    wrap_err!(configuration::refresh_network_data(None).await)
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
