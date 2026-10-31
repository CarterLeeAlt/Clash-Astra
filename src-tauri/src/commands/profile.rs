use super::CmdResult;
use crate::{
    application::{configuration, profiles},
    config::*,
    ret_err,
    utils::help,
    wrap_err,
};

#[tauri::command]
pub async fn get_profiles() -> CmdResult<serde_json::Value> {
    let snapshot = Config::snapshot();
    let ready = crate::core::CoreManager::global().is_core_ready();
    let providers_ready = ready
        && crate::core::clash_api::check_subscription_ready(&snapshot)
            .await
            .is_ok();
    let mut value = serde_json::to_value(&snapshot.profiles).map_err(|error| error.to_string())?;
    for item in value["items"].as_array_mut().into_iter().flatten() {
        if !matches!(item["type"].as_str(), Some("remote" | "local")) {
            continue;
        }
        let active = item["uid"].as_str() == snapshot.profiles.current.as_deref();
        item["validation_status"] = if !active {
            "Stored, not activated"
        } else if !ready {
            "Core not ready"
        } else if !providers_ready {
            "Providers not ready"
        } else {
            "Loaded and ready"
        }
        .into();
    }
    Ok(value)
}

#[tauri::command]
pub async fn enhance_profiles() -> CmdResult {
    wrap_err!(configuration::reload().await)
}

#[tauri::command]
pub async fn import_profile(url: String, option: Option<PrfOption>) -> CmdResult {
    wrap_err!(profiles::import(url, option).await)
}

#[tauri::command]
pub async fn reorder_profile(active_id: String, over_id: String) -> CmdResult {
    wrap_err!(profiles::reorder(active_id, over_id).await)
}

#[tauri::command]
pub async fn create_profile(item: PrfItem, file_data: Option<String>) -> CmdResult {
    wrap_err!(profiles::create(item, file_data).await)
}

#[tauri::command]
pub async fn update_profile(index: String, option: Option<PrfOption>) -> CmdResult {
    wrap_err!(profiles::update_profile(index, option).await)
}

#[tauri::command]
pub async fn delete_profile(index: String) -> CmdResult {
    wrap_err!(profiles::delete(index).await)
}

/// 修改profiles的
#[tauri::command]
pub async fn patch_profiles_config(
    profiles: crate::domain::profiles::ActivationRequest,
) -> CmdResult {
    wrap_err!(self::profiles::activate(profiles).await)
}

/// 修改某个profile item的
#[tauri::command]
pub async fn patch_profile(index: String, profile: PrfItem) -> CmdResult {
    wrap_err!(profiles::patch(index, profile).await)
}

#[tauri::command]
pub async fn view_profile(index: String) -> CmdResult {
    let file = {
        wrap_err!(Config::profiles().get_item(&index))?
            .file
            .clone()
            .ok_or("the file field is null")
    }?;

    let path = wrap_err!(help::resolve_profile_path(&file))?;
    let canonical_path = wrap_err!(dunce::canonicalize(&path))?;

    if !canonical_path.exists() {
        ret_err!("the file not found");
    }

    tokio::task::spawn_blocking(move || {
        wrap_err!(crate::infrastructure::desktop::open_file(canonical_path))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn read_profile_file(index: String) -> CmdResult<String> {
    let _operation = wrap_err!(crate::application::operation().await)?;
    let item = {
        let profiles = Config::profiles();
        wrap_err!(profiles.get_item(&index))?.clone()
    };

    tokio::task::spawn_blocking(move || wrap_err!(item.read_file()))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn save_profile_file(index: String, file_data: Option<String>) -> CmdResult {
    let Some(data) = file_data else {
        return Ok(());
    };

    wrap_err!(profiles::save_file(index, data).await)
}

#[tauri::command]
pub async fn select_proxy(profile_uid: String, group: String, proxy: String) -> CmdResult {
    wrap_err!(profiles::select_proxy(profile_uid, group, proxy).await)
}
