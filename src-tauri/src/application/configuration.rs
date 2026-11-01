use super::routing::{dispatch_switch, SwitchBudget};
use super::transaction::{self, TransactionDriver};
use crate::config::*;
use crate::core::{clash_api, handle::Handle, hotkey::Hotkey, sysopt::Sysopt, timer, CoreManager};
use crate::infrastructure::config_repository::{self as repository, FileTransaction, ProfileFiles};
use anyhow::{Context, Result};
use serde_yaml::Mapping;

#[derive(Clone, Debug)]
pub struct ProxySelection {
    pub group: String,
    pub node: String,
    pub previous: String,
}

pub struct Change {
    pub before: ConfigSnapshot,
    pub next: ConfigSnapshot,
    pub files: ProfileFiles,
    pub reload: bool,
    pub core_patch: Option<Mapping>,
    pub restore_selection: bool,
    pub close_connections: bool,
    pub selection: Option<ProxySelection>,
    pub switch_budget: Option<SwitchBudget>,
}

impl Change {
    pub fn new() -> Self {
        let before = (*Config::snapshot()).clone();
        Self::from_snapshot(before)
    }

    fn from_snapshot(before: ConfigSnapshot) -> Self {
        Self {
            next: before.clone(),
            before,
            files: ProfileFiles::new(),
            reload: false,
            core_patch: None,
            restore_selection: false,
            close_connections: false,
            selection: None,
            switch_budget: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeAction {
    None,
    Patch,
    Reload,
    Restart,
}

struct Driver {
    change: Change,
    files: Option<FileTransaction>,
    action: RuntimeAction,
    runtime_attempted: bool,
    proxy_attempted: bool,
    firewall_attempted: bool,
    launch_attempted: bool,
    hotkeys_attempted: bool,
    timer_attempted: bool,
    selection_attempted: bool,
    selection_before: Vec<(String, String)>,
    cleanup: Option<clash_api::ConnectionCleanup>,
    recovered: bool,
    recovery_required: bool,
}

fn changed<T: serde::Serialize>(before: &T, after: &T) -> bool {
    serde_json::to_value(before).ok() != serde_json::to_value(after).ok()
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .context("configuration worker failed")?
}

impl Driver {
    fn new(mut change: Change) -> Self {
        let action = runtime_action(&change, CoreManager::global().is_core_ready());
        // 重载和重启都会重建代理组，恢复有效的持久化选择属于该操作本身。
        change.restore_selection |=
            matches!(action, RuntimeAction::Reload | RuntimeAction::Restart);
        change.close_connections |= change.reload;
        Self {
            change,
            files: None,
            action,
            runtime_attempted: false,
            proxy_attempted: false,
            firewall_attempted: false,
            launch_attempted: false,
            hotkeys_attempted: false,
            timer_attempted: false,
            selection_attempted: false,
            selection_before: Vec::new(),
            cleanup: None,
            recovered: false,
            recovery_required: false,
        }
    }

    async fn apply_proxy(snapshot: ConfigSnapshot) -> Result<()> {
        blocking(move || Sysopt::global().apply_sysproxy(&snapshot.clash_settings, &snapshot.clash))
            .await
    }

    fn check_deadline(&self) -> Result<()> {
        self.change
            .switch_budget
            .map_or(Ok(()), SwitchBudget::check)
    }

    async fn verify_target(&self) -> Result<()> {
        if let Some(selection) = &self.change.selection {
            clash_api::verify_selection(
                self.change.next.clash.get_client_info(),
                &selection.group,
                &selection.node,
            )
            .await?;
        }
        if self.action != RuntimeAction::None || self.recovered {
            clash_api::verify_runtime(&self.change.next).await?;
        }
        Ok(())
    }

    async fn recover_core(&mut self) -> Result<()> {
        anyhow::ensure!(
            !self.recovered,
            "automatic core recovery already attempted once"
        );
        self.check_deadline()?;
        self.recovered = true;
        self.runtime_attempted = true;
        let deadline = self
            .change
            .switch_budget
            .map(|budget| budget.deadline)
            .unwrap_or_else(|| tokio::time::Instant::now() + std::time::Duration::from_secs(20));
        CoreManager::global()
            .run_core_with_until(&self.change.next, deadline)
            .await?;
        if let Some(selection) = &self.change.selection {
            self.change
                .switch_budget
                .context("selection deadline missing")?
                .run(clash_api::verify_selection(
                    self.change.next.clash.get_client_info(),
                    &selection.group,
                    &selection.node,
                ))
                .await?;
        }
        Ok(())
    }
}

/// 依据业务变化决定唯一的内核动作；组合请求只执行其中强度最高的一项。
fn runtime_action(change: &Change, core_ready: bool) -> RuntimeAction {
    let restart = ["external-controller", "secret"]
        .iter()
        .any(|key| change.before.clash.0.get(*key) != change.next.clash.0.get(*key))
        || change.before.clash_settings.clash_core != change.next.clash_settings.clash_core
        || change.before.clash_settings.enable_service_mode
            != change.next.clash_settings.enable_service_mode;
    if restart
        || ((change.reload || change.core_patch.is_some() || change.selection.is_some())
            && !core_ready)
    {
        RuntimeAction::Restart
    } else if change.reload {
        RuntimeAction::Reload
    } else if change.core_patch.is_some() {
        RuntimeAction::Patch
    } else {
        RuntimeAction::None
    }
}

impl TransactionDriver for Driver {
    async fn prepare(&mut self) -> Result<()> {
        self.check_deadline()?;
        let before = &self.change.before.clash_settings;
        let next = &self.change.next.clash_settings;
        if let Some(hotkeys) = &next.hotkeys {
            if before.hotkeys != next.hotkeys {
                crate::domain::hotkeys::parse(hotkeys)?;
            }
        }
        if before.enable_service_mode != next.enable_service_mode
            && before.enable_tun_mode == Some(true)
            && next.enable_tun_mode != Some(false)
        {
            anyhow::bail!("Disable TUN mode before changing service mode.");
        }
        let enabling_service =
            next.enable_service_mode == Some(true) && before.enable_service_mode != Some(true);
        let enabling_tun =
            next.enable_tun_mode == Some(true) && before.enable_tun_mode != Some(true);
        if enabling_service || enabling_tun {
            anyhow::ensure!(
                crate::core::win_service::is_current_process_elevated()?,
                "Service mode and TUN mode require administrator privileges."
            );
            anyhow::ensure!(
                crate::core::win_service::check_service().await?.installed,
                "Install the Windows service first."
            );
        }
        anyhow::ensure!(
            next.enable_tun_mode != Some(true) || next.enable_service_mode == Some(true),
            "TUN mode requires Service Mode."
        );
        if ["mixed-port", "socks-port", "port"]
            .iter()
            .any(|key| self.change.before.clash.0.get(*key) != self.change.next.clash.0.get(*key))
        {
            let before_ports = self.change.before.clash.get_client_info();
            let next_ports = self.change.next.clash.get_client_info();
            let owned_ports = [
                before_ports.mixed_port,
                before_ports.socks_port,
                before_ports.port,
            ];
            for port in [
                next_ports.mixed_port,
                next_ports.socks_port,
                next_ports.port,
            ] {
                if !owned_ports.contains(&port) {
                    anyhow::ensure!(
                        port_scanner::local_port_available(port),
                        "port {port} is already in use"
                    );
                }
            }
        }
        if (self.change.close_connections || self.change.selection.is_some())
            && CoreManager::global().is_core_ready()
        {
            let deadline = self
                .change
                .switch_budget
                .map(|budget| budget.recovery_at())
                .unwrap_or_else(|| tokio::time::Instant::now() + std::time::Duration::from_secs(4));
            let connections = super::routing::retry_until(deadline, || {
                clash_api::get_connections_for(self.change.before.clash.get_client_info())
            })
            .await;
            let edge = self
                .change
                .selection
                .as_ref()
                .map(|selection| (selection.group.clone(), selection.previous.clone()));
            match connections {
                Ok(connections) => {
                    self.cleanup = Some(clash_api::ConnectionCleanup::capture(&connections, edge))
                }
                Err(_) => self.recovery_required = true,
            }
        }
        if (self.change.restore_selection
            || self.change.selection.is_some()
            || self.action != RuntimeAction::None)
            && CoreManager::global().is_core_ready()
            && !self.recovery_required
        {
            let deadline = self
                .change
                .switch_budget
                .map(|budget| budget.recovery_at())
                .unwrap_or_else(|| tokio::time::Instant::now() + std::time::Duration::from_secs(4));
            let proxies = super::routing::retry_until(deadline, || {
                clash_api::get_proxies_for(self.change.before.clash.get_client_info())
            })
            .await;
            match proxies {
                Ok(proxies) => {
                    if let Some(selection) = &mut self.change.selection {
                        let target = proxies
                            .proxies
                            .get(&selection.group)
                            .context("proxy group no longer exists")?;
                        anyhow::ensure!(
                            matches!(target.proxy_type.as_str(), "Selector" | "Fallback")
                                && target
                                    .all
                                    .as_ref()
                                    .is_some_and(|all| all.contains(&selection.node)),
                            "proxy selection is not valid for this group"
                        );
                        selection.previous = target.now.clone().unwrap_or_default();
                        if selection.previous == selection.node {
                            self.cleanup = None;
                        } else if let Some(cleanup) = &mut self.cleanup {
                            cleanup
                                .set_old_edge(selection.group.clone(), selection.previous.clone());
                        }
                    }
                    self.selection_before = proxies
                        .proxies
                        .into_values()
                        .filter(|proxy| {
                            matches!(proxy.proxy_type.as_str(), "Selector" | "Fallback")
                        })
                        .filter_map(|proxy| proxy.now.map(|node| (proxy.name, node)))
                        .collect();
                }
                Err(_) => self.recovery_required = true,
            }
        }
        if self.action == RuntimeAction::Patch {
            if let Some(patch) = &self.change.core_patch {
                let runtime = self
                    .change
                    .next
                    .runtime
                    .config
                    .get_or_insert_with(Mapping::new);
                for (key, value) in patch {
                    runtime.insert(key.clone(), value.clone());
                }
            }
        } else if self.action != RuntimeAction::None {
            let candidate = self.change.next.clone();
            let files = self.change.files.clone();
            let runtime = blocking(move || repository::compile(&candidate, &files)).await?;
            for message in &runtime.diagnostics {
                Handle::emit_log("info", message);
            }
            self.change.next.runtime = runtime;
            let candidate = self.change.next.clone();
            blocking(move || CoreManager::global().check_config(&candidate)).await?;
        }
        self.check_deadline()?;
        let candidate = self.change.next.clone();
        let files = self.change.files.clone();
        self.files = Some(blocking(move || repository::prepare(&candidate, &files)).await?);
        Ok(())
    }

    async fn apply(&mut self) -> Result<()> {
        self.check_deadline()?;
        self.files
            .as_ref()
            .context("configuration transaction not prepared")?
            .apply()?;
        if self.recovery_required
            || (self.action == RuntimeAction::Restart && self.change.switch_budget.is_some())
        {
            self.recover_core().await?;
        }
        let before = &self.change.before;
        let next = &self.change.next;
        if before.clash.0.get("allow-lan") != next.clash.0.get("allow-lan") {
            self.firewall_attempted = true;
            let enabled = next
                .clash
                .0
                .get("allow-lan")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            blocking(move || crate::core::win_firewall::set_lan_firewall_enabled(enabled)).await?;
        }
        self.runtime_attempted |= self.action != RuntimeAction::None;
        let action = async {
            if self.recovered {
                return Ok(());
            }
            match self.action {
                RuntimeAction::None => {}
                RuntimeAction::Restart => CoreManager::global().run_core_with(next).await?,
                RuntimeAction::Reload => CoreManager::global().update_config(next).await?,
                RuntimeAction::Patch => {
                    if let Some(budget) = self.change.switch_budget {
                        super::routing::retry_until(budget.recovery_at(), || async {
                            clash_api::patch_configs_for(
                                before.clash.get_client_info(),
                                self.change.core_patch.as_ref().unwrap(),
                            )
                            .await?;
                            clash_api::verify_runtime(next).await
                        })
                        .await?;
                    } else {
                        clash_api::patch_configs_for(
                            before.clash.get_client_info(),
                            self.change.core_patch.as_ref().unwrap(),
                        )
                        .await?;
                    }
                }
            };
            Ok::<_, anyhow::Error>(())
        };
        if let Some(budget) = self.change.switch_budget {
            let first_deadline = budget.recovery_at().max(tokio::time::Instant::now());
            let result = tokio::time::timeout_at(first_deadline, action).await;
            if !matches!(result, Ok(Ok(()))) {
                self.recover_core().await?;
            }
        } else {
            action.await?;
        }
        if matches!(self.action, RuntimeAction::Reload | RuntimeAction::Restart) && !self.recovered
        {
            clash_api::wait_subscription_ready(&self.change.next).await?;
        }
        if let Some(selection) = self
            .change
            .selection
            .clone()
            .filter(|selection| !self.recovered && selection.previous != selection.node)
        {
            self.selection_attempted = true;
            let budget = self
                .change
                .switch_budget
                .context("node selection requires an operation deadline")?;
            let apply = clash_api::select_proxy_until(
                self.change.next.clash.get_client_info(),
                &selection.group,
                &selection.node,
                budget.recovery_at(),
            );
            if !matches!(
                tokio::time::timeout_at(budget.recovery_at(), apply).await,
                Ok(Ok(()))
            ) {
                self.recover_core().await?;
            }
        } else if self.change.restore_selection && !self.recovered {
            self.selection_attempted = true;
            super::profiles::restore_selected(&self.change.next).await?;
        }
        if self.action != RuntimeAction::None {
            let verify = clash_api::verify_runtime(&self.change.next);
            if let Some(budget) = self.change.switch_budget {
                budget.run(verify).await?;
            } else {
                verify.await?;
            }
        }
        if self.action == RuntimeAction::Reload {
            clash_api::verify_tun_state(&self.change.next).await?;
        }
        let before = &self.change.before;
        let next = &self.change.next;
        if before.clash_settings.enable_system_proxy != next.clash_settings.enable_system_proxy
            || before.clash_settings.system_proxy_bypass != next.clash_settings.system_proxy_bypass
            || before.clash.get_mixed_port() != next.clash.get_mixed_port()
        {
            self.proxy_attempted = true;
            Self::apply_proxy(next.clone()).await?;
        }
        if before.clash_settings.enable_auto_launch != next.clash_settings.enable_auto_launch {
            self.launch_attempted = true;
            let enabled = next.clash_settings.enable_auto_launch.unwrap_or(false);
            blocking(move || Sysopt::global().apply_launch(enabled)).await?;
        }
        if before.clash_settings.hotkeys != next.clash_settings.hotkeys {
            self.hotkeys_attempted = true;
            Hotkey::global().update(next.clash_settings.hotkeys.clone().unwrap_or_default())?;
        }
        if before.clash_settings.app_log_level != next.clash_settings.app_log_level {
            crate::bootstrap::logging::set_log_level(next.clash_settings.get_log_level())?;
        }
        if changed(&before.profiles, &next.profiles) {
            self.check_deadline()?;
            self.timer_attempted = true;
            timer::sync_with(&next.profiles)?;
        }
        if self.change.selection.is_none() {
            if let Some(cleanup) = &mut self.cleanup {
                cleanup.include_started_before_now();
            }
        }
        Ok(())
    }

    async fn commit(&mut self) -> Result<()> {
        self.check_deadline()?;
        self.files
            .as_mut()
            .context("configuration transaction not prepared")?
            .commit()
    }

    async fn complete(&mut self) -> Result<()> {
        self.check_deadline()?;
        if let Some(cleanup) = self
            .cleanup
            .clone()
            .filter(|_| self.action != RuntimeAction::Restart && !self.recovered)
        {
            let deadline = self
                .change
                .switch_budget
                .map(|budget| budget.recovery_at())
                .unwrap_or_else(|| tokio::time::Instant::now() + std::time::Duration::from_secs(4));
            super::routing::cleanup_or_recover(
                &cleanup,
                self.change.next.clash.get_client_info(),
                deadline,
                self.recover_core(),
            )
            .await?;
        }
        if self.action != RuntimeAction::None || self.change.selection.is_some() || self.recovered {
            let deadline = self
                .change
                .switch_budget
                .map(|budget| {
                    if self.recovered {
                        budget.deadline
                    } else {
                        budget.recovery_at()
                    }
                })
                .unwrap_or_else(|| tokio::time::Instant::now() + std::time::Duration::from_secs(4));
            if let Err(error) = super::routing::retry_until(deadline, || self.verify_target()).await
            {
                if self.recovered || self.action == RuntimeAction::Restart {
                    return Err(error);
                }
                self.recover_core().await?;
            }
        }
        self.check_deadline()
    }

    async fn rollback(&mut self) -> Result<()> {
        let mut errors = Vec::new();
        if let Some(files) = &self.files {
            if let Err(error) = files.restore() {
                errors.push(error.to_string());
            }
        }
        let before = &self.change.before;
        if self
            .change
            .switch_budget
            .is_some_and(|budget| budget.check().is_err())
        {
            // Files may finish restoring on a blocking worker, but an expired switch
            // must never issue a late API mutation over a newer user request.
            if errors.is_empty() {
                if let Some(files) = &self.files {
                    files.finish_rollback()?;
                }
            }
            if self.runtime_attempted || self.selection_attempted {
                Handle::refresh_clash();
                anyhow::bail!("switch deadline expired; runtime state requires readback, no late rollback was issued; file recovery errors: {}", errors.join("; "));
            }
            anyhow::ensure!(errors.is_empty(), "{}", errors.join("; "));
            return Ok(());
        }
        if self.runtime_attempted {
            let restore = async {
                if before.runtime.config.is_none() {
                    // 首次配置失败时没有旧内核可恢复，应停止候选进程，而不是用空配置重启。
                    tokio::time::timeout(
                        std::time::Duration::from_secs(10),
                        CoreManager::global().stop_core_async(),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        Err(anyhow::anyhow!(
                            "stopping the uncommitted initial core timed out"
                        ))
                    })
                } else if self.change.switch_budget.is_some() {
                    // A bounded switch may restore through APIs, never consume a
                    // second process restart while compensating a failed recovery.
                    let patch = before
                        .runtime
                        .config
                        .as_ref()
                        .context("previous runtime is missing")?;
                    clash_api::patch_configs_for(before.clash.get_client_info(), patch).await?;
                    clash_api::verify_runtime(before).await
                } else if self.action == RuntimeAction::Restart || self.recovered {
                    CoreManager::global().run_core_with(before).await
                } else {
                    CoreManager::global().update_config(before).await
                }
            };
            let result = if let Some(budget) = self.change.switch_budget {
                budget.run(restore).await
            } else {
                restore.await
            };
            if let Err(error) = result {
                errors.push(format!("restore core: {error:#}"));
            }
        }
        if self.selection_attempted || self.runtime_attempted {
            for (group, node) in &self.selection_before {
                let restore =
                    clash_api::select_proxy_for(before.clash.get_client_info(), group, node);
                let result = if let Some(budget) = self.change.switch_budget {
                    budget.run(restore).await
                } else {
                    restore.await
                };
                if let Err(error) = result {
                    errors.push(format!("restore selection: {error:#}"));
                }
            }
        }
        if self.proxy_attempted {
            if let Err(error) = Self::apply_proxy(before.clone()).await {
                errors.push(format!("restore system proxy: {error:#}"));
            }
        }
        if self.firewall_attempted {
            let enabled = before
                .clash
                .0
                .get("allow-lan")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if let Err(error) =
                blocking(move || crate::core::win_firewall::set_lan_firewall_enabled(enabled)).await
            {
                errors.push(format!("restore firewall: {error:#}"));
            }
        }
        if self.launch_attempted {
            let enabled = before.clash_settings.enable_auto_launch.unwrap_or(false);
            if let Err(error) = blocking(move || Sysopt::global().apply_launch(enabled)).await {
                errors.push(format!("restore auto-launch: {error:#}"));
            }
        }
        if self.hotkeys_attempted {
            if let Err(error) =
                Hotkey::global().update(before.clash_settings.hotkeys.clone().unwrap_or_default())
            {
                errors.push(format!("restore hotkeys: {error:#}"));
            }
        }
        if before.clash_settings.app_log_level != self.change.next.clash_settings.app_log_level {
            if let Err(error) =
                crate::bootstrap::logging::set_log_level(before.clash_settings.get_log_level())
            {
                errors.push(error.to_string());
            }
        }
        if self.timer_attempted {
            if let Err(error) = timer::sync_with(&before.profiles) {
                errors.push(format!("restore schedules: {error:#}"));
            }
        }
        anyhow::ensure!(errors.is_empty(), "{}", errors.join("; "));
        if let Some(files) = &self.files {
            files.finish_rollback()?;
        }
        if self.runtime_attempted {
            Handle::refresh_clash();
        }
        Ok(())
    }

    fn publish(&mut self) {
        let runtime_changed =
            self.action != RuntimeAction::None || self.change.selection.is_some() || self.recovered;
        if runtime_changed {
            self.change.next.runtime_revision = self.change.before.runtime_revision + 1;
        }
        let profiles_changed = changed(&self.change.before.profiles, &self.change.next.profiles);
        let mut before_preferences = self.change.before.clash_settings.clone();
        let mut next_preferences = self.change.next.clash_settings.clone();
        before_preferences.window_size_position = None;
        before_preferences.window_is_maximized = None;
        next_preferences.window_size_position = None;
        next_preferences.window_is_maximized = None;
        let settings_changed = changed(&before_preferences, &next_preferences);
        let revision = Config::publish(self.change.next.clone());
        if self.change.before.clash_settings.clash_core
            != self.change.next.clash_settings.clash_core
        {
            crate::logging::buffer::Logger::global().clear_log();
        }
        if runtime_changed || settings_changed {
            Sysopt::global().guard_proxy();
        }
        if self.change.before.clash_settings.language != self.change.next.clash_settings.language {
            crate::log_err!(Handle::update_systray());
        } else if runtime_changed || settings_changed {
            crate::log_err!(Handle::update_systray_part());
        }
        if runtime_changed || settings_changed || profiles_changed {
            Handle::configuration_committed(
                revision,
                runtime_changed,
                profiles_changed,
                settings_changed,
            );
        }
        if runtime_changed {
            tauri::async_runtime::spawn(async {
                if let Err(error) = super::diagnostics::run_auto_delay_test().await {
                    log::warn!(target: "app", "automatic delay test after commit: {error:#}");
                }
            });
        }
    }
}

pub async fn apply_locked(mut change: Change) -> Result<()> {
    // 每个订阅独立计版本；下载期间的主题或窗口修改不会误判为订阅冲突。
    let mut versions = std::collections::BTreeMap::new();
    for item in change.next.profiles.items.iter().flatten() {
        let Some(uid) = &item.uid else {
            continue;
        };
        let before = change.before.profiles.get_item(uid).ok();
        let contents_changed = item
            .file
            .as_ref()
            .is_some_and(|file| change.files.contains_key(file));
        let edited = contents_changed || before.is_none_or(|before| changed(before, item));
        let version = change
            .before
            .profile_versions
            .get(uid)
            .copied()
            .unwrap_or(0)
            + u64::from(edited);
        versions.insert(uid.clone(), version);
    }
    change.next.profile_versions = versions;
    let stop_idle_service = change.before.clash_settings.enable_service_mode == Some(true)
        && change.next.clash_settings.enable_service_mode != Some(true);
    transaction::execute(&mut Driver::new(change)).await?;
    if stop_idle_service {
        if let Err(error) = crate::core::win_service::stop_service_if_idle().await {
            log::warn!(target: "app", "service mode disabled; idle service shutdown failed: {error:#}");
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn initialize_test_snapshot(snapshot: ConfigSnapshot) {
    // Test builds resolve app_home_dir exclusively inside workspace .tmp.
    // Initial test state still enters through the coordinator's publish boundary.
    Config::publish(snapshot);
}

pub async fn mutate(edit: impl FnOnce(&mut Change) -> Result<()> + Send + 'static) -> Result<()> {
    super::dispatch(async move {
        let _operation = super::operation().await?;
        let mut change = Change::new();
        edit(&mut change)?;
        apply_locked(change).await
    })
    .await
}

pub async fn patch_clash(patch: Mapping) -> Result<()> {
    patch_clash_until(patch, None).await
}

pub async fn patch_clash_until(patch: Mapping, deadline_unix_ms: Option<u64>) -> Result<()> {
    crate::domain::network::validate_patch(&patch)?;
    if patch.len() == 1 && patch.contains_key("mode") {
        let budget = SwitchBudget::from_deadline_unix_ms(deadline_unix_ms);
        return dispatch_switch(budget, async move {
            let _operation = budget.run(super::operation()).await?;
            let mut change = Change::new();
            change.switch_budget = Some(budget);
            edit_clash(&mut change, patch)?;
            apply_locked(change).await
        })
        .await;
    }
    mutate(move |change| edit_clash(change, patch)).await
}

/// 内核端口与应用代理端口在候选阶段同步，所有副作用读取同一份目标快照。
fn edit_clash(change: &mut Change, patch: Mapping) -> Result<()> {
    change.close_connections = patch
        .get("mode")
        .is_some_and(|mode| change.before.clash.0.get("mode") != Some(mode));
    change.close_connections |= ["mixed-port", "socks-port", "port", "allow-lan"]
        .iter()
        .any(|key| {
            patch
                .get(*key)
                .is_some_and(|value| change.before.clash.0.get(*key) != Some(value))
        });
    for (key, value) in &patch {
        if key.as_str() == Some("tun") {
            let mut tun = change
                .next
                .clash
                .0
                .get("tun")
                .and_then(|value| value.as_mapping())
                .cloned()
                .unwrap_or_default();
            for (key, value) in value.as_mapping().context("tun must be a mapping")? {
                tun.insert(key.clone(), value.clone());
            }
            change.next.clash.0.insert(key.clone(), tun.into());
        } else {
            change.next.clash.0.insert(key.clone(), value.clone());
        }
    }
    if patch.get("mixed-port").is_some() {
        change.next.clash_settings.clash_mixed_port = Some(change.next.clash.get_mixed_port());
    }
    if patch.get("socks-port").is_some() {
        change.next.clash_settings.clash_socks_port =
            Some(IClashTemp::guard_socks_port(&change.next.clash.0));
    }
    if patch.get("port").is_some() {
        change.next.clash_settings.clash_port = Some(IClashTemp::guard_port(&change.next.clash.0));
    }
    let ports = crate::domain::network::PortSettings {
        mixed: change.next.clash.get_mixed_port(),
        socks: IClashTemp::guard_socks_port(&change.next.clash.0),
        http: IClashTemp::guard_port(&change.next.clash.0),
    };
    if ["mixed-port", "socks-port", "port"]
        .iter()
        .any(|key| patch.contains_key(*key))
    {
        ports.validate()?;
    }
    change.reload = patch
        .keys()
        .filter_map(serde_yaml::Value::as_str)
        .any(|key| {
            !matches!(
                key,
                "mode"
                    | "log-level"
                    | "mixed-port"
                    | "socks-port"
                    | "port"
                    | "allow-lan"
                    | "external-controller"
                    | "secret"
                    | "find-process-mode"
            )
        });
    change.close_connections |= change.reload;
    change.core_patch = Some(patch);
    Ok(())
}

pub async fn patch_ports(ports: crate::domain::network::PortSettings) -> Result<()> {
    patch_clash(ports.validate()?.mapping()).await
}

pub async fn patch_clash_settings(patch: IClashSettings) -> Result<()> {
    if let Some(core) = &patch.clash_core {
        anyhow::ensure!(
            VALID_MIHOMO_CORES.contains(&core.as_str()),
            "invalid Mihomo core"
        );
    }
    mutate(move |change| {
        change.reload = patch.enable_tun_mode.is_some();
        let random_changed = patch.enable_random_port.is_some_and(|enabled| {
            enabled
                != change
                    .before
                    .clash_settings
                    .enable_random_port
                    .unwrap_or(false)
        });
        let random_port = if random_changed {
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            Some(listener.local_addr()?.port())
        } else {
            None
        };
        let mut live_patch = Mapping::new();
        for (key, port) in [
            ("mixed-port", random_port.or(patch.clash_mixed_port)),
            ("socks-port", patch.clash_socks_port),
            ("port", patch.clash_port),
        ] {
            if let Some(port) = port {
                change.next.clash.0.insert(key.into(), port.into());
                live_patch.insert(key.into(), port.into());
            }
        }
        change.next.clash_settings.patch_config(patch);
        if let Some(port) = random_port {
            change.next.clash_settings.clash_mixed_port = Some(port);
        }
        if !live_patch.is_empty() {
            change.close_connections |= live_patch
                .iter()
                .any(|(key, value)| change.before.clash.0.get(key) != Some(value));
            change.core_patch = Some(live_patch);
        }
        crate::domain::network::validate_patch(&change.next.clash.0)?;
        Ok(())
    })
    .await
}

pub async fn reload() -> Result<()> {
    mutate(|change| {
        change.reload = true;
        change.restore_selection = true;
        Ok(())
    })
    .await
}

pub async fn refresh_network_data(provider: Option<(String, String)>) -> Result<()> {
    super::dispatch(async move {
        let _operation = super::operation().await?;
        let mut change = Change::new();
        let info = change.next.clash.get_client_info();
        match provider {
            Some((kind, name)) => clash_api::update_provider(info, &kind, &name).await?,
            None => clash_api::update_geo_data(info).await?,
        }
        change.reload = true;
        apply_locked(change).await
    })
    .await
}

/// 修复操作也必须持有业务许可直到阻塞写入结束，不能因 IPC 调用方消失而提前放锁。
pub async fn repair_system_proxy() -> Result<()> {
    super::dispatch(async {
        let _operation = super::operation().await?;
        blocking(|| Sysopt::global().update_sysproxy()).await?;
        Sysopt::global().guard_proxy();
        Ok(())
    })
    .await
}

/// 在异步业务入口开放前，完成启动配置生成与持久化。
pub fn initialize() -> Result<()> {
    let mut snapshot = (*Config::snapshot()).clone();
    let port = if snapshot.clash_settings.enable_random_port == Some(true) {
        std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port()
    } else {
        snapshot
            .clash_settings
            .clash_mixed_port
            .unwrap_or(snapshot.clash.get_mixed_port())
    };
    snapshot.clash_settings.clash_mixed_port = Some(port);
    snapshot.clash.0.insert("mixed-port".into(), port.into());
    snapshot.runtime = repository::compile(&snapshot, &ProfileFiles::new())?;
    let mut files = repository::prepare(&snapshot, &ProfileFiles::new())?;
    if let Err(error) = files.apply().and_then(|_| files.commit()) {
        files.rollback()?;
        return Err(error);
    }
    Config::publish(snapshot);
    Ok(())
}

pub fn save_window_state(position: Option<Vec<f64>>, maximized: bool) {
    tauri::async_runtime::spawn(async move {
        let result = persist_window_state(position, maximized).await;
        if let Err(error) = result {
            log::warn!(target: "app", "save window state: {error:#}");
        }
    });
}

pub async fn persist_window_state(position: Option<Vec<f64>>, maximized: bool) -> Result<()> {
    mutate(move |change| {
        change.next.clash_settings.window_is_maximized = Some(maximized);
        if let Some(position) = position {
            change.next.clash_settings.window_size_position = Some(position);
        }
        Ok(())
    })
    .await
}

pub fn clear_window_position() {
    tauri::async_runtime::spawn(async {
        crate::log_err!(
            mutate(|change| {
                change.next.clash_settings.window_size_position = None;
                Ok(())
            })
            .await
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Change {
        Change::from_snapshot(ConfigSnapshot {
            revision: 7,
            runtime_revision: 3,
            profile_versions: Default::default(),
            clash: IClashTemp::template(),
            clash_settings: IClashSettings::template(),
            profiles: IProfiles::template(),
            runtime: IRuntime::default(),
        })
    }

    #[test]
    fn all_ports_share_one_candidate_and_one_live_patch_decision() {
        let mut change = fixture();
        let patch = crate::domain::network::PortSettings {
            mixed: 18001,
            socks: 18002,
            http: 18003,
        }
        .mapping();
        edit_clash(&mut change, patch).unwrap();
        assert_eq!(runtime_action(&change, true), RuntimeAction::Patch);
        let target = change.next.clash.get_client_info();
        assert_eq!(
            (target.mixed_port, target.socks_port, target.port),
            (18001, 18002, 18003)
        );
        assert_eq!(
            change.next.clash_settings.clash_mixed_port,
            Some(target.mixed_port)
        );
        assert_eq!(
            change.next.clash_settings.clash_socks_port,
            Some(target.socks_port)
        );
        assert_eq!(change.next.clash_settings.clash_port, Some(target.port));
        assert_eq!(change.before.clash.get_mixed_port(), 7897);
        assert_eq!(change.before.clash_settings.clash_mixed_port, Some(7897));
        assert_eq!(change.before.revision, 7);
    }

    #[test]
    fn metadata_does_not_restart_and_reload_dominates_a_live_patch() {
        let mut change = fixture();
        change.next.clash_settings.language = Some("en".into());
        assert_eq!(runtime_action(&change, true), RuntimeAction::None);
        assert_eq!(runtime_action(&change, false), RuntimeAction::None);
        edit_clash(&mut change, serde_yaml::from_str("mode: global").unwrap()).unwrap();
        assert_eq!(runtime_action(&change, true), RuntimeAction::Patch);
        assert!(change.close_connections);
        change.reload = true;
        assert_eq!(runtime_action(&change, true), RuntimeAction::Reload);
        assert_eq!(runtime_action(&change, false), RuntimeAction::Restart);
    }

    #[test]
    fn tun_patch_merges_defaults_and_requires_reload() {
        let mut change = fixture();
        edit_clash(
            &mut change,
            serde_yaml::from_str("tun:\n  stack: system").unwrap(),
        )
        .unwrap();
        let tun = change.next.clash.0.get("tun").unwrap();
        assert_eq!(tun.get("stack").unwrap().as_str(), Some("system"));
        assert_eq!(tun.get("auto-route").unwrap().as_bool(), Some(true));
        assert_eq!(runtime_action(&change, true), RuntimeAction::Reload);
        assert_eq!(
            change
                .before
                .clash
                .0
                .get("tun")
                .unwrap()
                .get("stack")
                .unwrap()
                .as_str(),
            Some("gvisor")
        );
    }

    #[test]
    fn controller_and_hosting_changes_restart_but_log_and_mode_do_not() {
        let mut change = fixture();
        edit_clash(
            &mut change,
            serde_yaml::from_str("secret: synthetic-new").unwrap(),
        )
        .unwrap();
        assert_eq!(runtime_action(&change, true), RuntimeAction::Restart);
        let mut change = fixture();
        change.next.clash_settings.enable_service_mode = Some(true);
        assert_eq!(runtime_action(&change, true), RuntimeAction::Restart);
        let mut change = fixture();
        edit_clash(
            &mut change,
            serde_yaml::from_str("log-level: debug").unwrap(),
        )
        .unwrap();
        assert_eq!(runtime_action(&change, true), RuntimeAction::Patch);
        assert!(!change.close_connections);
    }
}
