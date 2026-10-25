use super::CmdResult;
use crate::{
    config::*,
    core::{clash_api, logger},
    feat, wrap_err,
};
use anyhow::Context;
use serde_yaml::Mapping;
use std::collections::{HashMap, VecDeque};

#[tauri::command]
pub fn get_clash_info() -> CmdResult<ClashInfo> {
    Ok(Config::clash().latest().get_client_info())
}

#[tauri::command]
pub fn get_runtime_config() -> CmdResult<Option<Mapping>> {
    Ok(Config::runtime().latest().config.clone())
}

#[tauri::command]
pub fn get_runtime_yaml() -> CmdResult<String> {
    let runtime = Config::runtime();
    let runtime = runtime.latest();
    let config = runtime.config.as_ref();
    wrap_err!(config
        .ok_or(anyhow::anyhow!("failed to parse config to yaml file"))
        .and_then(
            |config| serde_yaml::to_string(config).context("failed to convert config to yaml")
        ))
}

#[tauri::command]
pub fn get_runtime_exists() -> CmdResult<Vec<String>> {
    Ok(Config::runtime().latest().exists_keys.clone())
}

#[tauri::command]
pub fn get_runtime_logs() -> CmdResult<HashMap<String, Vec<(String, String)>>> {
    Ok(Config::runtime().latest().chain_logs.clone())
}

#[tauri::command]
pub async fn patch_clash_config(payload: Mapping) -> CmdResult {
    wrap_err!(feat::patch_clash(payload).await)
}

#[tauri::command]
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
        Err(err) => Err(err.to_string()),
    }
}

/// 最近一轮后端自动延迟测试的低值结果（节点名 -> 延迟毫秒）
#[tauri::command]
pub fn get_auto_delay_results() -> CmdResult<HashMap<String, u32>> {
    Ok(feat::latest_auto_delay_results())
}

/// 立即触发一轮自动延迟测试（切换、新建或更新订阅后调用）
#[tauri::command]
pub async fn trigger_auto_delay_test() -> CmdResult {
    match feat::run_auto_delay_test().await {
        Ok(_) => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}
