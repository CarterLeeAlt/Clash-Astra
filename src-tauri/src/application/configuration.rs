use super::transaction::{self, TransactionDriver};
use crate::config::*;
use crate::core::{clash_api, handle::Handle, hotkey::Hotkey, sysopt::Sysopt, timer, CoreManager};
use crate::infrastructure::config_repository::{self as repository, FileTransaction, ProfileFiles};
use anyhow::{Context, Result};
use serde_yaml::Mapping;

pub struct Change {
    pub before: ConfigSnapshot,
    pub next: ConfigSnapshot,
    pub files: ProfileFiles,
    pub reload: bool,
    pub core_patch: Option<Mapping>,
    pub restore_selection: bool,
    pub close_connections: bool,
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
        }
    }

    async fn apply_proxy(snapshot: ConfigSnapshot) -> Result<()> {
        blocking(move || Sysopt::global().apply_sysproxy(&snapshot.verge, &snapshot.clash)).await
    }
}

/// 依据业务变化决定唯一的内核动作；组合请求只执行其中强度最高的一项。
fn runtime_action(change: &Change, core_ready: bool) -> RuntimeAction {
    let restart = [
        "mixed-port",
        "socks-port",
        "port",
        "external-controller",
        "secret",
        "allow-lan",
    ]
    .iter()
    .any(|key| change.before.clash.0.get(*key) != change.next.clash.0.get(*key))
        || change.before.verge.clash_core != change.next.verge.clash_core
        || change.before.verge.enable_service_mode != change.next.verge.enable_service_mode;
    if restart || ((change.reload || change.core_patch.is_some()) && !core_ready) {
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
        let before = &self.change.before.verge;
        let next = &self.change.next.verge;
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
        if self.action == RuntimeAction::Restart {
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
        if (self.change.restore_selection || self.action != RuntimeAction::None)
            && CoreManager::global().is_core_ready()
        {
            self.selection_before =
                clash_api::get_proxies_for(self.change.before.clash.get_client_info())
                    .await?
                    .proxies
                    .into_values()
                    .filter(|proxy| matches!(proxy.proxy_type.as_str(), "Selector" | "Fallback"))
                    .filter_map(|proxy| proxy.now.map(|node| (proxy.name, node)))
                    .collect();
        }
        if self.action != RuntimeAction::None {
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
        let candidate = self.change.next.clone();
        let files = self.change.files.clone();
        self.files = Some(blocking(move || repository::prepare(&candidate, &files)).await?);
        Ok(())
    }

    async fn apply(&mut self) -> Result<()> {
        self.files
            .as_ref()
            .context("configuration transaction not prepared")?
            .apply()?;
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
        self.runtime_attempted = self.action != RuntimeAction::None;
        match self.action {
            RuntimeAction::None => {}
            RuntimeAction::Restart => CoreManager::global().run_core_with(next).await?,
            RuntimeAction::Reload => CoreManager::global().update_config(next).await?,
            RuntimeAction::Patch => {
                clash_api::patch_configs_for(
                    next.clash.get_client_info(),
                    self.change.core_patch.as_ref().unwrap(),
                )
                .await?
            }
        }
        if matches!(self.action, RuntimeAction::Reload | RuntimeAction::Restart) {
            clash_api::wait_subscription_ready(next).await?;
        }
        if self.change.restore_selection {
            self.selection_attempted = true;
            super::profiles::restore_selected(next).await?;
        }
        if before.verge.enable_system_proxy != next.verge.enable_system_proxy
            || before.verge.system_proxy_bypass != next.verge.system_proxy_bypass
            || before.clash.get_mixed_port() != next.clash.get_mixed_port()
        {
            self.proxy_attempted = true;
            Self::apply_proxy(next.clone()).await?;
        }
        if before.verge.enable_auto_launch != next.verge.enable_auto_launch {
            self.launch_attempted = true;
            let enabled = next.verge.enable_auto_launch.unwrap_or(false);
            blocking(move || Sysopt::global().apply_launch(enabled)).await?;
        }
        if before.verge.hotkeys != next.verge.hotkeys {
            self.hotkeys_attempted = true;
            Hotkey::global().update(next.verge.hotkeys.clone().unwrap_or_default())?;
        }
        if changed(&before.profiles, &next.profiles) {
            self.timer_attempted = true;
            timer::sync_with(&next.profiles)?;
        }
        Ok(())
    }

    async fn commit(&mut self) -> Result<()> {
        self.files
            .as_mut()
            .context("configuration transaction not prepared")?
            .commit()
    }

    async fn rollback(&mut self) -> Result<()> {
        let mut errors = Vec::new();
        if let Some(files) = &self.files {
            if let Err(error) = files.restore() {
                errors.push(error.to_string());
            }
        }
        let before = &self.change.before;
        if self.runtime_attempted {
            let result = if before.runtime.config.is_none() {
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
            } else if self.action == RuntimeAction::Restart {
                CoreManager::global().run_core_with(before).await
            } else {
                CoreManager::global().update_config(before).await
            };
            if let Err(error) = result {
                errors.push(format!("restore core: {error:#}"));
            }
        }
        if self.selection_attempted || self.runtime_attempted {
            for (group, node) in &self.selection_before {
                if let Err(error) =
                    clash_api::select_proxy_for(before.clash.get_client_info(), group, node).await
                {
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
            let enabled = before.verge.enable_auto_launch.unwrap_or(false);
            if let Err(error) = blocking(move || Sysopt::global().apply_launch(enabled)).await {
                errors.push(format!("restore auto-launch: {error:#}"));
            }
        }
        if self.hotkeys_attempted {
            if let Err(error) =
                Hotkey::global().update(before.verge.hotkeys.clone().unwrap_or_default())
            {
                errors.push(format!("restore hotkeys: {error:#}"));
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
        let runtime_changed = self.action != RuntimeAction::None;
        if runtime_changed {
            self.change.next.runtime_revision = self.change.before.runtime_revision + 1;
        }
        let profiles_changed = changed(&self.change.before.profiles, &self.change.next.profiles);
        let mut before_preferences = self.change.before.verge.clone();
        let mut next_preferences = self.change.next.verge.clone();
        before_preferences.window_size_position = None;
        before_preferences.window_is_maximized = None;
        next_preferences.window_size_position = None;
        next_preferences.window_is_maximized = None;
        let settings_changed = changed(&before_preferences, &next_preferences);
        let revision = Config::publish(self.change.next.clone());
        if self.change.before.verge.clash_core != self.change.next.verge.clash_core {
            crate::logging::buffer::Logger::global().clear_log();
        }
        if runtime_changed || settings_changed {
            Sysopt::global().guard_proxy();
        }
        if self.change.before.verge.language != self.change.next.verge.language {
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
        if self.action != RuntimeAction::None {
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
    let close_connections = change.close_connections;
    let stop_idle_service = change.before.verge.enable_service_mode == Some(true)
        && change.next.verge.enable_service_mode != Some(true);
    let controller = change.next.clash.get_client_info();
    transaction::execute(&mut Driver::new(change)).await?;
    if stop_idle_service {
        if let Err(error) = crate::core::win_service::stop_service_if_idle().await {
            log::warn!(target: "app", "service mode disabled; idle service shutdown failed: {error:#}");
        }
    }
    // 断开旧连接不可撤销，因此只在配置成功提交后执行。
    if close_connections {
        if let Err(error) = clash_api::close_all_connections_for(controller).await {
            log::warn!(target: "app", "configuration committed; closing previous connections failed: {error:#}");
        }
    }
    Ok(())
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
    crate::domain::network::validate_patch(&patch)?;
    mutate(move |change| edit_clash(change, patch)).await
}

/// 内核端口与应用代理端口在候选阶段同步，所有副作用读取同一份目标快照。
fn edit_clash(change: &mut Change, patch: Mapping) -> Result<()> {
    change.close_connections = patch
        .get("mode")
        .is_some_and(|mode| change.before.clash.0.get("mode") != Some(mode))
        && change.before.verge.auto_close_connection.unwrap_or(false);
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
        change.next.verge.verge_mixed_port = Some(change.next.clash.get_mixed_port());
    }
    if patch.get("socks-port").is_some() {
        change.next.verge.verge_socks_port =
            Some(IClashTemp::guard_socks_port(&change.next.clash.0));
    }
    if patch.get("port").is_some() {
        change.next.verge.verge_port = Some(IClashTemp::guard_port(&change.next.clash.0));
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
    change.reload = patch.contains_key("tun") || patch.contains_key("ipv6");
    change.core_patch = Some(patch);
    Ok(())
}

pub async fn patch_ports(ports: crate::domain::network::PortSettings) -> Result<()> {
    patch_clash(ports.validate()?.mapping()).await
}

pub async fn patch_verge(patch: IVerge) -> Result<()> {
    if let Some(core) = &patch.clash_core {
        anyhow::ensure!(
            VALID_MIHOMO_CORES.contains(&core.as_str()),
            "invalid Mihomo core"
        );
    }
    mutate(move |change| {
        change.reload = patch.enable_tun_mode.is_some();
        for (key, port) in [
            ("mixed-port", patch.verge_mixed_port),
            ("socks-port", patch.verge_socks_port),
            ("port", patch.verge_port),
        ] {
            if let Some(port) = port {
                change.next.clash.0.insert(key.into(), port.into());
            }
        }
        change.next.verge.patch_config(patch);
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
    let port = if snapshot.verge.enable_random_port == Some(true) {
        std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port()
    } else {
        snapshot
            .verge
            .verge_mixed_port
            .unwrap_or(snapshot.clash.get_mixed_port())
    };
    snapshot.verge.verge_mixed_port = Some(port);
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
        change.next.verge.window_is_maximized = Some(maximized);
        if let Some(position) = position {
            change.next.verge.window_size_position = Some(position);
        }
        Ok(())
    })
    .await
}

pub fn clear_window_position() {
    tauri::async_runtime::spawn(async {
        crate::log_err!(
            mutate(|change| {
                change.next.verge.window_size_position = None;
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
            verge: IVerge::template(),
            profiles: IProfiles::template(),
            runtime: IRuntime::default(),
        })
    }

    #[test]
    fn all_ports_share_one_candidate_and_one_restart_decision() {
        let mut change = fixture();
        let patch = crate::domain::network::PortSettings {
            mixed: 18001,
            socks: 18002,
            http: 18003,
        }
        .mapping();
        edit_clash(&mut change, patch).unwrap();
        assert_eq!(runtime_action(&change, true), RuntimeAction::Restart);
        let target = change.next.clash.get_client_info();
        assert_eq!(
            (target.mixed_port, target.socks_port, target.port),
            (18001, 18002, 18003)
        );
        assert_eq!(change.next.verge.verge_mixed_port, Some(target.mixed_port));
        assert_eq!(change.next.verge.verge_socks_port, Some(target.socks_port));
        assert_eq!(change.next.verge.verge_port, Some(target.port));
        assert_eq!(change.before.clash.get_mixed_port(), 7897);
        assert_eq!(change.before.verge.verge_mixed_port, Some(7897));
        assert_eq!(change.before.revision, 7);
    }

    #[test]
    fn metadata_does_not_restart_and_reload_dominates_a_live_patch() {
        let mut change = fixture();
        change.next.verge.language = Some("en".into());
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
}
