use super::configuration::{mutate, Change};
use crate::config::*;
use crate::core::clash_api;
use crate::domain::profiles as model;
use anyhow::{Context, Result};

fn items(change: &mut Change) -> &mut Vec<PrfItem> {
    change.next.profiles.items.get_or_insert_with(Vec::new)
}

async fn append(mut item: PrfItem, activate_first: bool) -> Result<()> {
    mutate(move |change| {
        let uid = item.uid.clone().context("profile UID is missing")?;
        anyhow::ensure!(
            !items(change)
                .iter()
                .any(|existing| existing.uid.as_ref() == Some(&uid)),
            "duplicate profile UID"
        );
        let file = item.file.clone().context("profile file name is missing")?;
        crate::utils::help::resolve_profile_path(&file)?;
        if let Some(data) = item.file_data.take() {
            change.files.insert(file, Some(data));
        }
        let activate = activate_first
            && change.next.profiles.current.is_none()
            && model::is_activatable(&item);
        items(change).push(item);
        if activate {
            model::select(&mut change.next.profiles, &uid)?;
            change.reload = true;
            change.restore_selection = true;
        }
        Ok(())
    })
    .await
}

pub async fn import(url: String, option: Option<PrfOption>) -> Result<()> {
    let item = PrfItem::from_url(&url, None, None, option).await?;
    append(item, true).await
}

pub async fn create(item: PrfItem, data: Option<String>) -> Result<()> {
    append(PrfItem::from(item, data).await?, false).await
}

pub async fn reorder(active: String, over: String) -> Result<()> {
    mutate(move |change| model::reorder(&mut change.next.profiles, &active, &over)).await
}

pub async fn delete(uid: String) -> Result<()> {
    mutate(move |change| {
        let active = change.next.profiles.current.as_ref() == Some(&uid);
        let in_chain = change
            .next
            .profiles
            .chain
            .iter()
            .flatten()
            .any(|item| item == &uid);
        let removed = model::remove(&mut change.next.profiles, &uid)?;
        if let Some(file) = removed.file {
            if !items(change)
                .iter()
                .any(|item| item.file.as_ref() == Some(&file))
            {
                change.files.insert(file, None);
            }
        }
        change.reload = active || in_chain;
        change.restore_selection = active;
        Ok(())
    })
    .await
}

pub async fn activate(patch: model::ActivationRequest) -> Result<()> {
    mutate(move |change| {
        change.close_connections = patch.current.is_some();
        if let Some(uid) = patch.current {
            model::select(&mut change.next.profiles, &uid)?;
        }
        if let Some(chain) = patch.chain {
            for uid in &chain {
                anyhow::ensure!(
                    uid != GLOBAL_SCRIPT_UID,
                    "Global Script is not a user enhancement"
                );
                let item = change.next.profiles.get_item(uid)?;
                anyhow::ensure!(
                    matches!(item.itype.as_deref(), Some("merge" | "script")),
                    "invalid enhancement chain item"
                );
            }
            change.next.profiles.chain = Some(chain);
        }
        change.reload = true;
        change.restore_selection = true;
        Ok(())
    })
    .await
}

pub async fn patch(uid: String, patch: PrfItem) -> Result<()> {
    mutate(move |change| {
        anyhow::ensure!(
            uid != GLOBAL_SCRIPT_UID,
            "Global Script identity cannot be modified"
        );
        let active = change.next.profiles.current.as_ref() == Some(&uid);
        let item = items(change)
            .iter_mut()
            .find(|item| item.uid.as_ref() == Some(&uid))
            .context("profile no longer exists")?;
        let previous_type = item.itype.clone();
        let previous_name = item.name.clone();
        let previous_file = item.file.clone();
        macro_rules! field {
            ($name:ident) => {
                if patch.$name.is_some() {
                    item.$name = patch.$name;
                }
            };
        }
        field!(itype);
        field!(name);
        field!(desc);
        field!(url);
        field!(selected);
        field!(option);
        if let Some(file) = patch.file {
            crate::utils::help::resolve_profile_path(&file)?;
            item.file = Some(file);
        }
        anyhow::ensure!(
            !active || model::is_activatable(item),
            "current subscription must remain remote or local"
        );
        anyhow::ensure!(
            matches!(
                item.itype.as_deref(),
                Some("remote" | "local" | "merge" | "script")
            ),
            "invalid profile type"
        );
        let content_changed =
            previous_type != item.itype || previous_name != item.name || previous_file != item.file;
        let in_chain = change
            .next
            .profiles
            .chain
            .iter()
            .flatten()
            .any(|item| item == &uid);
        if in_chain {
            anyhow::ensure!(
                matches!(
                    change.next.profiles.get_item(&uid)?.itype.as_deref(),
                    Some("merge" | "script")
                ),
                "disable the enhancement before changing its type"
            );
        }
        change.reload = content_changed && (active || in_chain);
        change.restore_selection = change.reload;
        Ok(())
    })
    .await
}

pub async fn update_profile(uid: String, option: Option<PrfOption>) -> Result<()> {
    let snapshot = Config::snapshot();
    let original = snapshot.profiles.get_item(&uid)?.clone();
    let expected_version = snapshot.profile_versions.get(&uid).copied().unwrap_or(0);
    if original.itype.as_deref() != Some("remote") {
        return super::configuration::reload().await;
    }
    let url = original
        .url
        .clone()
        .context("remote subscription URL is missing")?;
    let downloaded =
        PrfItem::from_url(&url, None, None, PrfOption::merge(original.option, option)).await?;
    mutate(move |change| {
        anyhow::ensure!(
            change
                .before
                .profile_versions
                .get(&uid)
                .copied()
                .unwrap_or(0)
                == expected_version,
            "subscription changed while downloading; retry"
        );
        let active = change.next.profiles.current.as_ref() == Some(&uid);
        let item = items(change)
            .iter_mut()
            .find(|item| item.uid.as_ref() == Some(&uid))
            .context("subscription was deleted while downloading")?;
        anyhow::ensure!(
            item.url.as_ref() == Some(&url),
            "subscription URL changed while downloading; retry"
        );
        let file = item
            .file
            .clone()
            .context("subscription file name is missing")?;
        item.extra = downloaded.extra;
        item.updated = downloaded.updated;
        // 更新响应可首次提供间隔；一次性的下载代理选项不能改写已保存偏好。
        if item
            .option
            .as_ref()
            .and_then(|option| option.update_interval)
            .is_none()
        {
            if let Some(interval) = downloaded
                .option
                .as_ref()
                .and_then(|option| option.update_interval)
            {
                item.option
                    .get_or_insert_with(PrfOption::default)
                    .update_interval = Some(interval);
            }
        }
        if let Some(data) = downloaded.file_data {
            change.files.insert(file, Some(data));
        }
        change.reload = active;
        change.restore_selection = active;
        Ok(())
    })
    .await
}

pub async fn save_file(uid: String, data: String) -> Result<()> {
    mutate(move |change| {
        let item = change.next.profiles.get_item(&uid)?.clone();
        let file = item.file.clone().context("profile file name is missing")?;
        if item.is_global_script() {
            let current = crate::infrastructure::config_repository::current_mapping(
                &change.next,
                &change.files,
            )?;
            let name = change
                .next
                .profiles
                .current
                .as_ref()
                .and_then(|uid| change.next.profiles.get_item(uid).ok())
                .and_then(|item| item.name.clone())
                .unwrap_or_default();
            crate::enhance::script::validate_script_strict(data.clone(), current, &name)?;
        }
        change.files.insert(file, Some(data));
        change.reload = item.is_global_script()
            || change.next.profiles.current.as_ref() == Some(&uid)
            || change
                .next
                .profiles
                .chain
                .iter()
                .flatten()
                .any(|item| item == &uid);
        change.restore_selection = change.reload;
        Ok(())
    })
    .await
}

pub async fn restore_selected(snapshot: &ConfigSnapshot) -> Result<()> {
    let Some(uid) = &snapshot.profiles.current else {
        return Ok(());
    };
    let item = snapshot.profiles.get_item(uid)?;
    let Some(selected) = &item.selected else {
        return Ok(());
    };
    if selected.is_empty() {
        return Ok(());
    }
    let info = snapshot.clash.get_client_info();
    let proxies = clash_api::get_proxies_for(info.clone()).await?.proxies;
    for selected in selected {
        let (Some(group), Some(node)) = (&selected.name, &selected.now) else {
            continue;
        };
        let Some(proxy) = proxies.get(group) else {
            continue;
        };
        if !matches!(proxy.proxy_type.as_str(), "Selector" | "Fallback")
            || !proxy.all.as_ref().is_some_and(|all| all.contains(node))
        {
            continue;
        }
        if proxy.now.as_ref() != Some(node) {
            clash_api::select_proxy_verified(info.clone(), group, node).await?;
        }
    }
    Ok(())
}

pub async fn select_proxy(
    profile_uid: String,
    group: String,
    proxy: String,
    deadline_unix_ms: Option<u64>,
) -> Result<()> {
    let budget = super::routing::SwitchBudget::from_deadline_unix_ms(deadline_unix_ms);
    super::routing::dispatch_switch(budget, async move {
        let _operation = budget.run(super::operation()).await?;
        let mut change = Change::new();
        anyhow::ensure!(
            change.next.profiles.current.as_ref() == Some(&profile_uid),
            "current subscription changed; refresh the proxy groups"
        );
        let uid = change
            .next
            .profiles
            .current
            .clone()
            .context("no current subscription")?;
        let item = items(&mut change)
            .iter_mut()
            .find(|item| item.uid.as_ref() == Some(&uid))
            .context("current subscription not found")?;
        let selected = item.selected.get_or_insert_with(Vec::new);
        selected.retain(|item| item.name.as_ref() != Some(&group));
        selected.push(PrfSelected {
            name: Some(group.clone()),
            now: Some(proxy.clone()),
        });
        change.selection = Some(super::configuration::ProxySelection {
            group,
            node: proxy,
            previous: String::new(),
        });
        change.switch_budget = Some(budget);
        super::configuration::apply_locked(change).await
    })
    .await
}
