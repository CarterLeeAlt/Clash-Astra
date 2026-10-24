use super::CmdResult;
use crate::{
    config::*,
    core::{handle, timer, CoreManager},
    feat, ret_err,
    utils::help,
    wrap_err,
};
use anyhow::{bail, Context, Result};

#[tauri::command]
pub fn get_profiles() -> CmdResult<IProfiles> {
    Ok(Config::profiles().data().clone())
}

#[tauri::command]
pub async fn enhance_profiles() -> CmdResult {
    wrap_err!(CoreManager::global().update_config().await)?;
    handle::Handle::refresh_clash();
    Ok(())
}

#[tauri::command]
pub async fn import_profile(url: String, option: Option<PrfOption>) -> CmdResult {
    let item = wrap_err!(PrfItem::from_url(&url, None, None, option).await)?;
    wrap_err!(Config::profiles().data().append_item(item))
}

#[tauri::command]
pub async fn reorder_profile(active_id: String, over_id: String) -> CmdResult {
    wrap_err!(Config::profiles().data().reorder(active_id, over_id))
}

#[tauri::command]
pub async fn create_profile(item: PrfItem, file_data: Option<String>) -> CmdResult {
    let item = wrap_err!(PrfItem::from(item, file_data).await)?;
    wrap_err!(Config::profiles().data().append_item(item))
}

#[tauri::command]
pub async fn update_profile(index: String, option: Option<PrfOption>) -> CmdResult {
    wrap_err!(feat::update_profile(index, option).await)
}

#[tauri::command]
pub async fn delete_profile(index: String) -> CmdResult {
    let should_update = wrap_err!({ Config::profiles().data().delete_item(index) })?;
    if should_update {
        wrap_err!(CoreManager::global().update_config().await)?;
        handle::Handle::refresh_clash();
    }

    Ok(())
}

/// 修改profiles的
#[tauri::command]
pub async fn patch_profiles_config(profiles: IProfiles) -> CmdResult {
    wrap_err!({ Config::profiles().draft().patch_config(profiles) })?;

    match CoreManager::global().update_config().await {
        Ok(_) => {
            // 先落盘再 apply：apply 之后再存盘失败会让内存与文件不一致，
            // 前端收到错误却已经生效，重启后又回跳
            if let Err(err) = Config::profiles().draft().save_file() {
                Config::profiles().discard();
                log::error!(target: "app", "{err}");
                return Err(format!("{err}"));
            }
            handle::Handle::refresh_clash();
            Config::profiles().apply();
            Ok(())
        }
        Err(err) => {
            Config::profiles().discard();
            log::error!(target: "app", "{err}");
            Err(format!("{err}"))
        }
    }
}

/// 修改某个profile item的
#[tauri::command]
pub fn patch_profile(index: String, profile: PrfItem) -> CmdResult {
    wrap_err!(Config::profiles().data().patch_item(index, profile))?;
    wrap_err!(timer::sync())
}

#[tauri::command]
pub fn view_profile(app_handle: tauri::AppHandle, index: String) -> CmdResult {
    let file = {
        wrap_err!(Config::profiles().latest().get_item(&index))?
            .file
            .clone()
            .ok_or("the file field is null")
    }?;

    let path = wrap_err!(help::resolve_profile_path(&file))?;
    let canonical_path = wrap_err!(dunce::canonicalize(&path))?;

    if !canonical_path.exists() {
        ret_err!("the file not found");
    }

    wrap_err!(help::open_file(app_handle, canonical_path))
}

#[tauri::command]
pub fn read_profile_file(index: String) -> CmdResult<String> {
    let profiles = Config::profiles();
    let profiles = profiles.latest();
    let item = wrap_err!(profiles.get_item(&index))?;
    let data = wrap_err!(item.read_file())?;
    Ok(data)
}

#[tauri::command]
pub async fn save_profile_file(index: String, file_data: Option<String>) -> CmdResult {
    let Some(data) = file_data else {
        return Ok(());
    };

    let item = {
        let profiles = Config::profiles();
        let profiles = profiles.latest();
        wrap_err!(profiles.get_item(&index))?.clone()
    };

    if item.is_global_script() {
        wrap_err!(save_global_script_file(item, data).await)
    } else {
        wrap_err!(item.save_file(data))
    }
}

async fn save_global_script_file(item: PrfItem, data: String) -> Result<()> {
    let (current_config, profile_name) = {
        let profiles = Config::profiles();
        let profiles = profiles.latest();
        let current_uid = profiles.get_current();
        let profile_name = current_uid
            .as_ref()
            .and_then(|uid| profiles.get_item(uid).ok())
            .and_then(|item| item.name.clone())
            .unwrap_or_default();
        (profiles.current_mapping().unwrap_or_default(), profile_name)
    };

    crate::enhance::script::validate_script_strict(data.clone(), current_config, &profile_name)
        .context("global script validation failed")?;

    let old_data = item.read_file()?;
    item.save_file(data)?;

    match CoreManager::global().update_config().await {
        Ok(_) => {
            handle::Handle::refresh_clash();
            Ok(())
        }
        Err(err) => {
            let rollback_err = item.save_file(old_data).err();
            let recovery_err = match CoreManager::global().update_config().await {
                Ok(_) => {
                    handle::Handle::refresh_clash();
                    None
                }
                Err(recovery_err) => {
                    log::error!(target: "app", "failed to recover config after global script rollback: {recovery_err}");
                    Some(recovery_err)
                }
            };
            match (rollback_err, recovery_err) {
                (Some(rollback_err), Some(recovery_err)) => bail!(
                    "global script rejected: {err}; rollback failed: {rollback_err}; recovery update failed: {recovery_err}"
                ),
                (Some(rollback_err), None) => {
                    bail!("global script rejected: {err}; rollback failed: {rollback_err}")
                }
                (None, Some(recovery_err)) => bail!(
                    "global script rejected: {err}; rollback restored the old file but recovery update failed: {recovery_err}"
                ),
                (None, None) => bail!("global script rejected: {err}"),
            }
        }
    }
}
