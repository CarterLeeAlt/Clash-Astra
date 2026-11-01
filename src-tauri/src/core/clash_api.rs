use crate::config::{ClashInfo, Config, DEFAULT_LATENCY_TEST_URL};
use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Mapping;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use tokio::time::Duration;

/// Clash 控制器的共享 HTTP 客户端：连接轮询很频繁，不能每次新建客户端。
static API_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::ClientBuilder::new()
        .no_proxy()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(8))
        .build()
        .expect("failed to build Clash API client")
});

/// PUT /configs
/// path 是绝对路径
pub async fn put_configs_for(info: ClashInfo, path: &str) -> Result<()> {
    let (url, headers) = build_clash_client_info(info)?;
    let url = format!("{url}/configs");

    let mut data = HashMap::new();
    data.insert("path", path);

    let client = reqwest::ClientBuilder::new()
        .no_proxy()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(8))
        .build()?;
    let builder = client
        .put(&url)
        .query(&[("force", "true")])
        .headers(headers)
        .json(&data);
    let response = builder.send().await?;

    match response.status().as_u16() {
        204 => Ok(()),
        status => {
            bail!("failed to put configs with status \"{status}\"")
        }
    }
}

/// PATCH /configs
pub async fn patch_configs_for(info: ClashInfo, config: &Mapping) -> Result<()> {
    let (url, headers) = build_clash_client_info(info)?;
    let url = format!("{url}/configs");

    let client = reqwest::ClientBuilder::new()
        .no_proxy()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(8))
        .build()?;
    let builder = client.patch(&url).headers(headers.clone()).json(config);
    builder.send().await?.error_for_status()?;
    Ok(())
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct ProxyItemRes {
    pub name: String,
    #[serde(default, rename = "type")]
    pub proxy_type: String,
    #[serde(default)]
    pub all: Option<Vec<String>>,
    #[serde(default)]
    pub now: Option<String>,
    #[serde(default)]
    pub selected: Option<String>,
    #[serde(default)]
    pub fixed: Option<String>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct ProxiesRes {
    #[serde(default)]
    pub proxies: HashMap<String, ProxyItemRes>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionMetadataRes {
    #[serde(default)]
    pub network: String,
    #[serde(default, rename = "type")]
    pub conn_type: Option<String>,
    #[serde(default)]
    pub host: String,
    #[serde(default, rename = "destinationIP")]
    pub destination_ip: Option<String>,
    #[serde(default, rename = "destinationPort")]
    pub destination_port: String,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionItemRes {
    pub id: String,
    #[serde(default)]
    pub metadata: ConnectionMetadataRes,
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub chains: Vec<String>,
    #[serde(default)]
    pub rule: Option<String>,
    #[serde(default, rename = "rulePayload")]
    pub rule_payload: Option<String>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionsRes {
    #[serde(deserialize_with = "connection_list")]
    pub connections: Vec<ConnectionItemRes>,
}

// Mihomo v1.19.33 serializes an empty tracker slice as null.
fn connection_list<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<ConnectionItemRes>, D::Error> {
    Ok(Option::<Vec<ConnectionItemRes>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct DelayRes {
    pub delay: u64,
}

/// 解析延迟测试 URL，为空时回落到默认地址。
fn resolve_latency_test_url(test_url: Option<String>) -> String {
    test_url
        .and_then(|url| {
            let trimmed = url.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .unwrap_or_else(|| DEFAULT_LATENCY_TEST_URL.to_string())
}

/// GET /proxies/{name}/delay
/// 获取代理延迟
pub async fn get_proxy_delay(
    name: String,
    test_url: Option<String>,
    timeout: i32,
) -> Result<DelayRes> {
    get_proxy_delay_for(Config::clash().get_client_info(), name, test_url, timeout).await
}

pub async fn get_proxy_delay_for(
    info: ClashInfo,
    name: String,
    test_url: Option<String>,
    timeout: i32,
) -> Result<DelayRes> {
    let (base_url, headers) = build_clash_client_info(info)?;
    let encoded_name = utf8_percent_encode(&name, NON_ALPHANUMERIC).to_string();
    let url = format!("{base_url}/proxies/{encoded_name}/delay");

    let test_url = resolve_latency_test_url(test_url);

    let timeout = if timeout <= 0 {
        5000
    } else if timeout > 60000 {
        60000
    } else {
        timeout
    };

    let timeout_param = format!("{timeout}");
    let request = API_CLIENT
        .get(&url)
        .headers(headers)
        .query(&[("timeout", &timeout_param), ("url", &test_url)])
        .timeout(Duration::from_millis(timeout as u64 + 5000))
        .build()
        .map_err(|err| {
            anyhow::anyhow!(
                "failed to build Clash proxy delay request for '{name}' via {url}: {err}"
            )
        })?;
    let request_url = request.url().clone();
    let request_path_and_query = request_url
        .query()
        .map(|query| format!("{}?{query}", request_url.path()))
        .unwrap_or_else(|| request_url.path().to_string());

    log::debug!("requesting Clash proxy delay: proxy={name}, request={request_path_and_query}");

    let response = API_CLIENT.execute(request).await.map_err(|err| {
        anyhow::anyhow!(
            "failed to request Clash proxy delay for '{name}' via {request_path_and_query}: {err}"
        )
    })?;

    let status = response.status();
    let body = response.text().await.map_err(|err| {
        anyhow::anyhow!(
            "failed to read Clash proxy delay response for '{name}' from {request_path_and_query}: {err}"
        )
    })?;

    log::debug!(
        "Clash proxy delay response: proxy={name}, request={request_path_and_query}, status={status}, body={body}"
    );

    if !status.is_success() {
        bail!(
            "Clash proxy delay request failed for '{name}' via {request_path_and_query} with status {status}: {body}"
        );
    }

    serde_json::from_str::<DelayRes>(&body).map_err(|err| {
        anyhow::anyhow!(
            "failed to parse Clash proxy delay response for '{name}' as JSON: {err}; body: {body}"
        )
    })
}

/// GET /proxies
pub async fn get_proxies() -> Result<ProxiesRes> {
    get_proxies_for(Config::clash().get_client_info()).await
}

pub async fn get_proxies_for(info: ClashInfo) -> Result<ProxiesRes> {
    let (url, headers) = build_clash_client_info(info)?;
    let url = format!("{url}/proxies");

    let response = API_CLIENT.get(&url).headers(headers).send().await?;

    Ok(response.error_for_status()?.json::<ProxiesRes>().await?)
}

/// 控制器可访问不代表 provider 已成功下载；所有被引用的集合都须实际加载。
pub async fn check_subscription_ready(snapshot: &crate::config::ConfigSnapshot) -> Result<()> {
    if snapshot.profiles.current.is_none() {
        return Ok(());
    }
    let config = snapshot
        .runtime
        .config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("runtime configuration is missing"))?;
    let (url, headers) = build_clash_client_info(snapshot.clash.get_client_info())?;
    for (field, endpoint, proxy_provider) in [
        ("proxy-providers", "providers/proxies", true),
        ("rule-providers", "providers/rules", false),
    ] {
        let Some(expected) = config
            .get(field)
            .and_then(serde_yaml::Value::as_mapping)
            .filter(|providers| !providers.is_empty())
        else {
            continue;
        };
        let response = API_CLIENT
            .get(format!("{url}/{endpoint}"))
            .headers(headers.clone())
            .timeout(Duration::from_secs(3))
            .send()
            .await?
            .error_for_status()?;
        let data: serde_json::Value = response.json().await?;
        for name in expected.keys().filter_map(serde_yaml::Value::as_str) {
            let actual = &data["providers"][name];
            validate_loaded_provider(actual, proxy_provider)?;
        }
    }
    Ok(())
}

pub async fn wait_subscription_ready(snapshot: &crate::config::ConfigSnapshot) -> Result<()> {
    let check = async {
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            match check_subscription_ready(snapshot).await {
                Ok(()) => return Ok(()),
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => tokio::time::sleep(Duration::from_millis(300)).await,
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(15), check)
        .await
        .map_err(|_| anyhow::anyhow!("subscription providers did not become ready within 15s"))?
}

fn validate_loaded_provider(actual: &serde_json::Value, proxy_provider: bool) -> Result<()> {
    anyhow::ensure!(actual.is_object(), "subscription provider is not loaded");
    if proxy_provider {
        anyhow::ensure!(
            actual["proxies"]
                .as_array()
                .is_some_and(|nodes| !nodes.is_empty()),
            "subscription proxy-provider has no loaded nodes"
        );
    } else {
        anyhow::ensure!(
            actual["ruleCount"].as_u64().is_some_and(|count| count > 0),
            "subscription rule-provider has no loaded rules"
        );
    }
    Ok(())
}

#[test]
fn provider_readiness_does_not_accept_missing_or_empty_content() {
    use serde_json::json;
    assert!(validate_loaded_provider(&json!(null), true).is_err());
    assert!(validate_loaded_provider(&json!({"proxies": []}), true).is_err());
    assert!(validate_loaded_provider(&json!({"ruleCount": 0}), false).is_err());
    assert!(validate_loaded_provider(&json!({"proxies": [{"name": "synthetic"}]}), true).is_ok());
    assert!(validate_loaded_provider(&json!({"ruleCount": 1}), false).is_ok());
}

pub async fn select_proxy_for(info: ClashInfo, group: &str, proxy: &str) -> Result<()> {
    let (url, headers) = build_clash_client_info(info)?;
    let group = utf8_percent_encode(group, NON_ALPHANUMERIC);
    API_CLIENT
        .put(format!("{url}/proxies/{group}"))
        .headers(headers)
        .json(&serde_json::json!({ "name": proxy }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// GET /connections
pub async fn get_connections() -> Result<ConnectionsRes> {
    get_connections_for(Config::clash().get_client_info()).await
}

pub async fn get_connections_for(info: ClashInfo) -> Result<ConnectionsRes> {
    let (url, headers) = build_clash_client_info(info)?;
    let url = format!("{url}/connections");

    let response = API_CLIENT.get(&url).headers(headers).send().await?;

    Ok(response
        .error_for_status()?
        .json::<ConnectionsRes>()
        .await?)
}

pub async fn get_configs_for(info: ClashInfo) -> Result<serde_json::Value> {
    let (url, headers) = build_clash_client_info(info)?;
    let mut actual: serde_json::Value = API_CLIENT
        .get(format!("{url}/configs"))
        .headers(headers)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    // The pinned API marshals TUN enums as gVisor/System/Mixed and omits false
    // strict-route. Expose the same canonical values used by the config editor.
    if let Some(tun) = actual
        .get_mut("tun")
        .and_then(serde_json::Value::as_object_mut)
    {
        if let Some(stack) = tun.get("stack").and_then(serde_json::Value::as_str) {
            tun.insert("stack".into(), stack.to_ascii_lowercase().into());
        }
        tun.entry("strict-route").or_insert(false.into());
    }
    Ok(actual)
}

pub async fn update_provider(info: ClashInfo, kind: &str, name: &str) -> Result<()> {
    anyhow::ensure!(matches!(kind, "proxies" | "rules"), "invalid provider kind");
    let (url, headers) = build_clash_client_info(info)?;
    let name = utf8_percent_encode(name, NON_ALPHANUMERIC);
    API_CLIENT
        .put(format!("{url}/providers/{kind}/{name}"))
        .headers(headers)
        .timeout(Duration::from_secs(60))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn update_geo_data(info: ClashInfo) -> Result<()> {
    let (url, headers) = build_clash_client_info(info)?;
    API_CLIENT
        .post(format!("{url}/configs/geo"))
        .headers(headers)
        .timeout(Duration::from_secs(60))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn select_proxy_verified(info: ClashInfo, group: &str, node: &str) -> Result<()> {
    select_proxy_for(info.clone(), group, node).await?;
    verify_selection(info, group, node).await
}

/// A fallback Set may run a health check inside Mihomo. Wait for that request
/// rather than repeatedly submitting concurrent setters after 600 ms.
pub async fn select_proxy_until(
    info: ClashInfo,
    group: &str,
    node: &str,
    deadline: tokio::time::Instant,
) -> Result<()> {
    let mut attempt = 0;
    loop {
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "selection API budget expired"
        );
        let result =
            tokio::time::timeout_at(deadline, select_proxy_verified(info.clone(), group, node))
                .await;
        match result {
            Ok(Ok(())) => return Ok(()),
            Err(_) => bail!("selection request did not finish before recovery was required"),
            Ok(Err(error)) => {
                let wait = crate::application::routing::retry_delay(attempt);
                if tokio::time::Instant::now() + wait >= deadline {
                    return Err(error);
                }
                tokio::time::sleep(wait).await;
                attempt += 1;
            }
        }
    }
}

pub async fn verify_selection(info: ClashInfo, group: &str, node: &str) -> Result<()> {
    let proxies = get_proxies_for(info).await?;
    let actual = proxies
        .proxies
        .get(group)
        .ok_or_else(|| anyhow::anyhow!("proxy group disappeared: {group}"))?;
    anyhow::ensure!(actual.now.as_deref() == Some(node), "requested {group} -> {node}, actual selection is {}; an automatic group may have fallen back", actual.now.as_deref().unwrap_or("unknown"));
    Ok(())
}

/// Track IDs plus the old edge in a group chain; never close every connection
/// merely because another group happens to use the same leaf proxy.
#[derive(Clone, Debug, Default)]
pub struct ConnectionCleanup {
    ids: HashSet<String>,
    old_edge: Option<(String, String)>,
    started_before: Option<chrono::DateTime<chrono::Utc>>,
}

impl ConnectionCleanup {
    pub fn include_started_before_now(&mut self) {
        self.started_before = Some(chrono::Utc::now());
    }
    pub fn set_old_edge(&mut self, group: String, node: String) {
        self.old_edge = Some((group, node));
    }
    pub fn capture(connections: &ConnectionsRes, edge: Option<(String, String)>) -> Self {
        let ids = connections
            .connections
            .iter()
            .filter(|connection| {
                edge.as_ref()
                    .is_none_or(|(group, _)| connection.chains.contains(group))
            })
            .map(|connection| connection.id.clone())
            .collect();
        Self {
            ids,
            old_edge: edge,
            started_before: None,
        }
    }

    fn matches(&self, connection: &ConnectionItemRes) -> bool {
        self.ids.contains(&connection.id)
            || self.started_before.is_some_and(|cutoff| {
                chrono::DateTime::parse_from_rfc3339(&connection.start)
                    .is_ok_and(|start| start <= cutoff)
            })
            || self.old_edge.as_ref().is_some_and(|(group, old)| {
                connection
                    .chains
                    .windows(2)
                    .any(|edge| edge[0] == *old && edge[1] == *group)
            })
    }

    pub async fn finish(&self, info: ClashInfo, deadline: tokio::time::Instant) -> Result<()> {
        let mut attempt = 0;
        let mut last_error = String::new();
        loop {
            let round = async {
                let pending: Vec<_> = get_connections_for(info.clone())
                    .await?
                    .connections
                    .into_iter()
                    .filter(|connection| self.matches(connection))
                    .collect();
                if pending.is_empty() {
                    return Ok::<_, anyhow::Error>(true);
                }
                let (url, headers) = build_clash_client_info(info.clone())?;
                for chunk in pending.chunks(16) {
                    let mut tasks = tokio::task::JoinSet::new();
                    for connection in chunk {
                        let mut endpoint = reqwest::Url::parse(&format!("{url}/connections/"))?;
                        endpoint
                            .path_segments_mut()
                            .map_err(|_| anyhow::anyhow!("invalid controller URL"))?
                            .pop_if_empty()
                            .push(&connection.id);
                        let request = API_CLIENT.delete(endpoint).headers(headers.clone());
                        tasks.spawn(async move {
                            request.send().await?.error_for_status()?;
                            Ok::<_, reqwest::Error>(())
                        });
                    }
                    while let Some(result) = tasks.join_next().await {
                        // A failed DELETE is retried only if a readback still finds the ID.
                        if let Err(error) = result
                            .map_err(anyhow::Error::from)
                            .and_then(|result| result.map_err(anyhow::Error::from))
                        {
                            last_error = error.to_string();
                        }
                    }
                }
                Ok(false)
            };
            match tokio::time::timeout_at(deadline, round).await {
                Ok(Ok(true)) => return Ok(()),
                Ok(Ok(false)) => {},
                Ok(Err(error)) => last_error = error.to_string(),
                Err(_) => bail!("old connections could not be verified closed before the deadline: {last_error}"),
            }
            let wake =
                tokio::time::Instant::now() + crate::application::routing::retry_delay(attempt);
            if wake >= deadline {
                bail!("old connections remain after automatic retries: {last_error}");
            }
            tokio::time::sleep_until(wake).await;
            attempt += 1;
        }
    }
}

pub async fn verify_runtime(snapshot: &crate::config::ConfigSnapshot) -> Result<()> {
    let actual = get_configs_for(snapshot.clash.get_client_info()).await?;
    let expected = snapshot
        .runtime
        .config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("runtime configuration is missing"))?;
    for key in [
        "mode",
        "log-level",
        "ipv6",
        "allow-lan",
        "mixed-port",
        "socks-port",
        "port",
    ] {
        if let Some(value) = expected.get(key) {
            anyhow::ensure!(
                actual[key] == serde_json::to_value(value)?,
                "Mihomo did not apply {key}: expected {value:?}, actual {}",
                actual[key]
            );
        }
    }
    for key in ["mixed-port", "socks-port", "port"] {
        if let Some(port) = expected
            .get(key)
            .and_then(serde_yaml::Value::as_u64)
            .filter(|port| *port > 0)
        {
            tokio::time::timeout(
                Duration::from_millis(500),
                tokio::net::TcpStream::connect(("127.0.0.1", port as u16)),
            )
            .await
            .map_err(|_| anyhow::anyhow!("{key} listener readiness timed out"))??;
        }
    }
    if let Some(tun) = expected.get("tun") {
        for key in [
            "enable",
            "stack",
            "device",
            "mtu",
            "auto-route",
            "strict-route",
            "auto-detect-interface",
            "dns-hijack",
        ] {
            if let Some(value) = tun.get(key) {
                // Disabled TUN has no active adapter parameters to read back.
                if key != "enable"
                    && tun.get("enable").and_then(serde_yaml::Value::as_bool) != Some(true)
                {
                    continue;
                }
                anyhow::ensure!(
                    actual["tun"][key] == {
                        let mut expected = serde_json::to_value(value)?;
                        if key == "dns-hijack" {
                            if let Some(entries) = expected.as_array_mut() {
                                entries.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                            }
                        }
                        expected
                    },
                    "Mihomo did not apply tun.{key}"
                );
            }
        }
    }
    Ok(())
}

pub async fn verify_tun_state(snapshot: &crate::config::ConfigSnapshot) -> Result<()> {
    #[cfg(windows)]
    if let Some(tun) = snapshot
        .runtime
        .config
        .as_ref()
        .and_then(|config| config.get("tun"))
    {
        if tun
            .get("device")
            .and_then(serde_yaml::Value::as_str)
            .is_some_and(|device| !device.is_empty())
        {
            let ipv6 = snapshot
                .runtime
                .config
                .as_ref()
                .and_then(|config| config.get("ipv6"))
                .and_then(serde_yaml::Value::as_bool)
                .unwrap_or(false);
            crate::infrastructure::tun_state::verify(tun, ipv6).await?;
        }
    }
    Ok(())
}

#[test]
fn connection_cleanup_is_scoped_to_group_and_preserves_new_paths() {
    let connection = |id: &str, chain: &[&str]| ConnectionItemRes {
        id: id.into(),
        chains: chain.iter().map(|name| name.to_string()).collect(),
        ..Default::default()
    };
    let before = ConnectionsRes {
        connections: vec![
            connection("old", &["A", "group"]),
            connection("unrelated", &["A", "other"]),
        ],
    };
    let cleanup = ConnectionCleanup::capture(&before, Some(("group".into(), "A".into())));
    assert!(cleanup.matches(&before.connections[0]));
    assert!(!cleanup.matches(&before.connections[1]));
    assert!(cleanup.matches(&connection("in-flight", &["A", "group"])));
    assert!(!cleanup.matches(&connection("new", &["B", "group"])));
}

#[test]
fn mode_cleanup_includes_racing_old_sessions_but_preserves_sessions_after_application() {
    let mut cleanup = ConnectionCleanup::capture(&ConnectionsRes::default(), None);
    cleanup.include_started_before_now();
    let before = ConnectionItemRes {
        id: "racing".into(),
        start: (chrono::Utc::now() - chrono::Duration::seconds(1)).to_rfc3339(),
        ..Default::default()
    };
    let after = ConnectionItemRes {
        id: "new".into(),
        start: (chrono::Utc::now() + chrono::Duration::seconds(1)).to_rfc3339(),
        ..Default::default()
    };
    assert!(cleanup.matches(&before));
    assert!(!cleanup.matches(&after));
}

#[cfg(test)]
mod switch_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };
    use warp::Filter;

    #[test]
    fn empty_core_tracker_is_null_but_an_absent_connection_snapshot_is_not_success() {
        assert!(
            serde_json::from_str::<ConnectionsRes>(r#"{"connections":null}"#)
                .unwrap()
                .connections
                .is_empty()
        );
        assert!(
            serde_json::from_str::<ConnectionsRes>(r#"{"connections":[]}"#)
                .unwrap()
                .connections
                .is_empty()
        );
        assert!(serde_json::from_str::<ConnectionsRes>(r#"{}"#).is_err());
    }

    async fn controller(
        failures: usize,
        keep_connection: bool,
    ) -> (ClashInfo, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let deletes = Arc::new(AtomicUsize::new(0));
        let gone = Arc::new(AtomicBool::new(false));
        let read_state = gone.clone();
        let get = warp::get().and(warp::path("connections")).and(warp::path::end()).map(move || {
            let connections = if read_state.load(Ordering::SeqCst) { vec![] } else { vec![serde_json::json!({"id":"1234-5678-abcd", "chains":["A","Main"],"metadata":{"network":"udp"}})] };
            warp::reply::json(&serde_json::json!({"connections":connections}))
        });
        let count = deletes.clone();
        let remove = warp::delete()
            .and(warp::path("connections"))
            .and(warp::path::param::<String>())
            .map(move |id: String| {
                let attempt = count.fetch_add(1, Ordering::SeqCst);
                if id != "1234-5678-abcd" {
                    return warp::http::StatusCode::BAD_REQUEST;
                }
                if attempt < failures {
                    return warp::http::StatusCode::SERVICE_UNAVAILABLE;
                }
                if !keep_connection {
                    gone.store(true, Ordering::SeqCst);
                }
                warp::http::StatusCode::NO_CONTENT
            });
        let (address, server) = warp::serve(get.or(remove)).bind_ephemeral(([127, 0, 0, 1], 0));
        let info = ClashInfo {
            server: address.to_string(),
            ..Default::default()
        };
        (info, deletes, tokio::spawn(server))
    }

    #[tokio::test]
    async fn transient_delete_failures_are_retried_until_readback_is_empty() {
        let (info, deletes, server) = controller(2, false).await;
        let before = get_connections_for(info.clone()).await.unwrap();
        let cleanup = ConnectionCleanup::capture(&before, Some(("Main".into(), "A".into())));
        cleanup
            .finish(
                info.clone(),
                tokio::time::Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap();
        assert_eq!(deletes.load(Ordering::SeqCst), 3);
        assert!(get_connections_for(info)
            .await
            .unwrap()
            .connections
            .is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn tun_readback_uses_the_pinned_api_enum_and_omitted_false_contract() {
        let route = warp::path("configs").map(|| warp::reply::json(&serde_json::json!({"tun":{"enable":true,"stack":"gVisor","device":"synthetic","mtu":1400,"dns-hijack":["0.0.0.0:53","tcp://any:53"]}})));
        let (address, server) = warp::serve(route).bind_ephemeral(([127, 0, 0, 1], 0));
        let server = tokio::spawn(server);
        let mut snapshot = crate::config::ConfigSnapshot {
            revision: 0,
            runtime_revision: 0,
            profile_versions: Default::default(),
            clash: crate::config::IClashTemp::template(),
            clash_settings: crate::config::IClashSettings::template(),
            profiles: crate::config::IProfiles::template(),
            runtime: crate::config::IRuntime::default(),
        };
        snapshot
            .clash
            .0
            .insert("external-controller".into(), address.to_string().into());
        snapshot.runtime.config = Some(serde_yaml::from_str("tun: {enable: true, stack: gvisor, device: synthetic, mtu: 1400, strict-route: false, dns-hijack: ['tcp://any:53', '0.0.0.0:53']}").unwrap());
        verify_runtime(&snapshot).await.unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn false_success_delete_escalates_once_instead_of_accepting_http_204() {
        let (info, deletes, server) = controller(0, true).await;
        let before = get_connections_for(info.clone()).await.unwrap();
        let cleanup = ConnectionCleanup::capture(&before, None);
        let recovered = AtomicUsize::new(0);
        crate::application::routing::cleanup_or_recover(
            &cleanup,
            info,
            tokio::time::Instant::now() + Duration::from_millis(350),
            async {
                recovered.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert!(deletes.load(Ordering::SeqCst) >= 2);
        assert_eq!(recovered.load(Ordering::SeqCst), 1);
        server.abort();
    }
}

/// 使用本次操作的目标控制地址检查就绪，避免端口修改时误查旧地址。
pub async fn wait_for_core_ready(client_info: ClashInfo, ready_timeout: Duration) -> Result<()> {
    let (url, headers) = build_clash_client_info(client_info)?;
    let url = format!("{url}/configs");
    let client = reqwest::ClientBuilder::new()
        .no_proxy()
        .timeout(Duration::from_millis(1200))
        .build()?;
    let started = Instant::now();

    loop {
        let last_error = match client.get(&url).headers(headers.clone()).send().await {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => format!("controller returned HTTP {}", response.status()),
            Err(err) => err.to_string(),
        };

        if started.elapsed() >= ready_timeout {
            bail!(
                "Clash controller was not ready within {}s: {last_error}",
                ready_timeout.as_secs()
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// 根据clash info获取clash服务地址和请求头
fn build_clash_client_info(client: ClashInfo) -> Result<(String, HeaderMap)> {
    let server = format!("http://{}", client.server);

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse()?);

    if let Some(secret) = client.secret.filter(|secret| !secret.trim().is_empty()) {
        let secret = format!("Bearer {}", secret).parse()?;
        headers.insert("Authorization", secret);
    }

    Ok((server, headers))
}

/// 从 Mihomo 配置校验输出中提取简洁的错误信息。
///
/// 输出格式不受我们控制（截断、缺引号、字段顺序变化、多字节字符），切片一律用
/// `get`：release 是 panic=abort，越界会直接结束整个进程。
pub fn parse_check_output(log: String) -> String {
    let t = log.find("time=");
    let m = log.find("msg=");
    let mr = log.rfind('"');

    if let (Some(_), Some(m), Some(mr)) = (t, m, mr) {
        let e = match log.find("level=error msg=") {
            Some(e) => e + 17,
            None => m + 5,
        };

        if mr > m {
            if let Some(message) = log.get(e..mr) {
                return message.to_owned();
            }
        }
    }

    if let Some(l) = log.find("error=") {
        let start = l + 6;
        let end = log[start..]
            .find("path=")
            .map(|r| (start + r).saturating_sub(1))
            .unwrap_or(log.len());
        if let Some(message) = log.get(start..end.max(start)) {
            return message.trim_end().to_owned();
        }
    }

    log
}

#[test]
fn test_parse_check_output() {
    let str1 = r#"xxxx\n time="2022-11-18T20:42:58+08:00" level=error msg="proxy 0: 'alpn' expected type 'string', got unconvertible type '[]interface {}'""#;
    let str2 = r#"20:43:49 ERR [Config] configuration file test failed error=proxy 0: unsupport proxy type: hysteria path=xxx"#;
    let str3 = r#"
    "time="2022-11-18T21:38:01+08:00" level=info msg="Start initial configuration in progress"
    time="2022-11-18T21:38:01+08:00" level=error msg="proxy 0: 'alpn' expected type 'string', got unconvertible type '[]interface {}'"
    configuration file xxx\n
    "#;

    let res1 = parse_check_output(str1.into());
    let res2 = parse_check_output(str2.into());
    let res3 = parse_check_output(str3.into());

    println!("res1: {res1}");
    println!("res2: {res2}");
    println!("res3: {res3}");

    assert_eq!(res1, res3);
    assert_eq!(res2, "proxy 0: unsupport proxy type: hysteria");
}

#[test]
fn test_parse_check_output_never_panics_on_malformed_output() {
    let cases = [
        // 开引号就是最后一个引号（消息被截断）
        r#"time="x" level=error msg=""#,
        r#"time="x" msg="#,
        // path= 在 error= 之前、或紧贴 error=
        "path=a error=bad",
        "error=path=",
        "path=error=",
        // 多字节字符落在切片边界
        "time=\"x\" level=error msg=\"配置错误",
        "error=错误path=",
        "",
    ];
    for case in cases {
        let _ = parse_check_output(case.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_latency_test_url_uses_default_for_none() {
        assert_eq!(resolve_latency_test_url(None), DEFAULT_LATENCY_TEST_URL);
    }

    #[test]
    fn resolve_latency_test_url_uses_default_for_blank() {
        assert_eq!(
            resolve_latency_test_url(Some("  	  ".into())),
            DEFAULT_LATENCY_TEST_URL
        );
    }

    #[test]
    fn resolve_latency_test_url_trims_and_preserves_custom_url() {
        assert_eq!(
            resolve_latency_test_url(Some("  https://example.com/healthz  ".into())),
            "https://example.com/healthz"
        );
    }
}
