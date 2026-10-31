use crate::config::{ClashInfo, Config, DEFAULT_LATENCY_TEST_URL};
use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Mapping;
use std::collections::HashMap;
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
    let builder = client.put(&url).headers(headers).json(&data);
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
    #[serde(default)]
    pub connections: Vec<ConnectionItemRes>,
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

pub async fn close_connections_for_node(node: &str) -> Result<()> {
    let (url, headers) = clash_client_info()?;
    for connection in get_connections()
        .await?
        .connections
        .into_iter()
        .filter(|connection| connection.chains.iter().any(|name| name == node))
    {
        let id = utf8_percent_encode(&connection.id, NON_ALPHANUMERIC);
        API_CLIENT
            .delete(format!("{url}/connections/{id}"))
            .headers(headers.clone())
            .send()
            .await?
            .error_for_status()?;
    }
    Ok(())
}

pub async fn close_all_connections_for(info: ClashInfo) -> Result<()> {
    let (url, headers) = build_clash_client_info(info)?;
    API_CLIENT
        .delete(format!("{url}/connections"))
        .headers(headers)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// GET /connections
pub async fn get_connections() -> Result<ConnectionsRes> {
    let (url, headers) = clash_client_info()?;
    let url = format!("{url}/connections");

    let response = API_CLIENT.get(&url).headers(headers).send().await?;

    Ok(response
        .error_for_status()?
        .json::<ConnectionsRes>()
        .await?)
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
fn clash_client_info() -> Result<(String, HeaderMap)> {
    let client = Config::clash().get_client_info();

    build_clash_client_info(client)
}

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
