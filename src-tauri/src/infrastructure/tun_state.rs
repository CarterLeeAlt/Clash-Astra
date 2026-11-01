use anyhow::{Context, Result};
use serde_yaml::Value;
use std::process::Stdio;
use std::time::Duration;

/// Read Windows adapter/route state; never modify host networking as a readiness probe.
pub async fn verify(tun: &Value, ipv6: bool) -> Result<()> {
    let device = tun
        .get("device")
        .and_then(Value::as_str)
        .context("TUN device is missing")?;
    const QUERY: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$interfaces = @(Get-NetIPInterface -ErrorAction Stop | Where-Object { $_.InterfaceAlias -ceq $env:CLASH_VERIFY_TUN_DEVICE })
$connected = @($interfaces | Where-Object { $_.ConnectionState -eq 'Connected' })
$routes = @($connected | ForEach-Object { Get-NetRoute -InterfaceIndex $_.InterfaceIndex -AddressFamily $_.AddressFamily -ErrorAction Stop } | ForEach-Object { $_.DestinationPrefix })
@{connected=($connected.Count -gt 0); routes=$routes} | ConvertTo-Json -Compress
"#;
    let mut command = tokio::process::Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", QUERY])
        .env("CLASH_VERIFY_TUN_DEVICE", device)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .context("Windows TUN state query timed out")??;
    anyhow::ensure!(
        output.status.success(),
        "Windows TUN adapter/route query failed for {device}"
    );
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    verify_readback(tun, ipv6, &actual)
}

fn verify_readback(tun: &Value, ipv6: bool, actual: &serde_json::Value) -> Result<()> {
    if tun.get("enable").and_then(Value::as_bool) != Some(true) {
        anyhow::ensure!(
            actual["connected"] == false,
            "TUN adapter remained connected after disabling TUN"
        );
        return Ok(());
    }
    anyhow::ensure!(actual["connected"] == true, "TUN adapter is not connected");
    if tun.get("auto-route").and_then(Value::as_bool) == Some(true) {
        let routes = actual["routes"]
            .as_array()
            .context("TUN routes are missing")?;
        let custom = tun.get("route-address").and_then(Value::as_sequence);
        if let Some(custom) = custom.filter(|routes| !routes.is_empty()) {
            for prefix in custom.iter().filter_map(Value::as_str) {
                anyhow::ensure!(
                    routes.iter().any(|route| route.as_str() == Some(prefix)),
                    "TUN route {prefix} was not installed"
                );
            }
        } else {
            let has = |prefix: &str| routes.iter().any(|route| route.as_str() == Some(prefix));
            anyhow::ensure!(
                has("0.0.0.0/0") || (has("0.0.0.0/1") && has("128.0.0.0/1")),
                "TUN default routes were not installed"
            );
            if ipv6 {
                anyhow::ensure!(
                    has("::/0") || (has("::/1") && has("8000::/1")),
                    "TUN IPv6 default routes were not installed"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn route_readback_checks_both_families_and_disabled_adapter() {
    let tun = serde_yaml::from_str("enable: true\nauto-route: true").unwrap();
    let ipv4 = serde_json::json!({"connected":true,"routes":["0.0.0.0/1","128.0.0.0/1"]});
    assert!(verify_readback(&tun, false, &ipv4).is_ok());
    assert!(verify_readback(&tun, true, &ipv4).is_err());
    let dual = serde_json::json!({"connected":true,"routes":["0.0.0.0/0","::/0"]});
    assert!(verify_readback(&tun, true, &dual).is_ok());
    let disabled = serde_yaml::from_str("enable: false").unwrap();
    assert!(verify_readback(&disabled, false, &dual).is_err());
    assert!(verify_readback(&disabled, false, &serde_json::json!({"connected":false})).is_ok());
    let custom =
        serde_yaml::from_str("enable: true\nauto-route: true\nroute-address: [2001:db8::/32]")
            .unwrap();
    assert!(verify_readback(&custom, true, &dual).is_err());
}
