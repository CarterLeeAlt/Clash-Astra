use crate::{config::Config, core::win_service};
use anyhow::{Context, Result};

/// 服务安装与配置修改共用写入协调器；UAC 等待在阻塞线程池内进行。
pub async fn install() -> Result<()> {
    super::dispatch(async {
        let _operation = super::operation().await?;
        tokio::task::spawn_blocking(win_service::install_service)
            .await
            .context("service installation worker failed")?
    })
    .await
}

/// 先将核心交还普通进程模式，再卸载服务；部分失败时明确报告仍然有效的状态。
pub async fn uninstall() -> Result<()> {
    super::dispatch(async {
        let _operation = super::operation().await?;
        anyhow::ensure!(
            Config::verge().enable_tun_mode != Some(true),
            "Disable TUN mode before uninstalling the service."
        );
        if Config::verge().enable_service_mode == Some(true) {
            let mut change = super::configuration::Change::new();
            change.next.verge.enable_service_mode = Some(false);
            super::configuration::apply_locked(change).await?;
        }
        win_service::uninstall_service()
            .await
            .context("service mode is disabled; Windows service removal failed")
    })
    .await
}
