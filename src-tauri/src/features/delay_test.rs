use crate::config::*;
use crate::core::*;
use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::{AppHandle, ClipboardManager};
use tokio::time::{sleep, Duration, Instant};

/// copy env variable
pub fn copy_clash_env(app_handle: &AppHandle) {
    let port = { Config::verge().latest().verge_mixed_port.unwrap_or(7897) };
    let http_proxy = format!("http://127.0.0.1:{}", port);
    let socks5_proxy = format!("socks5://127.0.0.1:{}", port);

    let sh =
        format!("export https_proxy={http_proxy} http_proxy={http_proxy} all_proxy={socks5_proxy}");
    let cmd: String = format!("set http_proxy={http_proxy} \n set https_proxy={http_proxy}");
    let ps: String = format!("$env:HTTP_PROXY=\"{http_proxy}\"; $env:HTTPS_PROXY=\"{http_proxy}\"");

    let mut cliboard = app_handle.clipboard_manager();

    let env_type = { Config::verge().latest().env_type.clone() };
    let env_type = match env_type {
        Some(env_type) => env_type,
        None => "powershell".to_string(),
    };
    match env_type.as_str() {
        "bash" => cliboard.write_text(sh).unwrap_or_default(),
        "cmd" => cliboard.write_text(cmd).unwrap_or_default(),
        "powershell" => cliboard.write_text(ps).unwrap_or_default(),
        _ => log::error!(target: "app", "copy_clash_env: Invalid env type! {env_type}"),
    };
}

#[derive(Default, Debug, Clone, Serialize)]
pub struct TestDelayResult {
    pub delay: u32,
    pub proxy: Option<String>,
}

/// 最近一轮自动延迟测试的低值结果（节点名 -> 延迟毫秒）。
/// 每节点两并发取低值，mihomo history 的写入顺序不保证，展示以此快照为准。
static LATEST_AUTO_DELAY: Lazy<Mutex<HashMap<String, u32>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// 读取最近一轮自动延迟测试的低值结果。
pub fn latest_auto_delay_results() -> HashMap<String, u32> {
    LATEST_AUTO_DELAY.lock().clone()
}

/// 后台自动延迟测试：逐节点两并发取低值，结果同时写入内核 history
/// 与内存快照（前端窗口重新可见时经 `get_auto_delay_results` 优先展示）。
/// 不依赖前端 WebView，窗口隐藏或被节流时照常执行。
pub async fn run_auto_delay_test() -> Result<()> {
    /// 同一节点的并发测试轮数；取两轮中的较低值以抑制单次抖动
    const CONCURRENT_ROUNDS: usize = 2;
    /// 全局同时在测的节点数上限，避免控制器请求风暴
    const GLOBAL_CONCURRENCY: usize = 16;

    let (test_url, timeout) = {
        let verge = Config::verge();
        let verge = verge.latest();
        (
            verge.default_latency_test.clone(),
            verge.default_latency_timeout.unwrap_or(5000),
        )
    };

    let proxies = match clash_api::get_proxies().await {
        Ok(res) => res.proxies,
        Err(err) => bail!("auto delay test skipped; failed to list proxies: {err}"),
    };

    // 收集全部代理组（除 GLOBAL）内的节点并去重；REJECT 无网络路径不测
    let mut names: HashSet<String> = HashSet::new();
    for (group, proxy) in proxies.iter() {
        if proxy.all.is_none() || group.as_str() == "GLOBAL" {
            continue;
        }
        names.extend(
            proxy
                .all
                .iter()
                .flatten()
                .filter(|name| name.as_str() != "REJECT")
                .cloned(),
        );
    }

    if names.is_empty() {
        log::debug!(target: "app", "auto delay test skipped; no proxies to test");
        return Ok(());
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(GLOBAL_CONCURRENCY));
    let mut set = tokio::task::JoinSet::new();

    for name in names {
        let semaphore = semaphore.clone();
        let test_url = test_url.clone();
        set.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("auto delay test semaphore closed");

            debug_assert_eq!(CONCURRENT_ROUNDS, 2);
            let (first, second) = tokio::join!(
                clash_api::get_proxy_delay(name.clone(), test_url.clone(), timeout),
                clash_api::get_proxy_delay(name.clone(), test_url, timeout),
            );

            let best = [first, second]
                .into_iter()
                .filter_map(|result| result.ok().map(|res| res.delay as u32))
                .min();
            (name, best)
        });
    }

    let mut latest: HashMap<String, u32> = HashMap::new();
    while let Some(joined) = set.join_next().await {
        let Ok((name, best)) = joined else {
            log::warn!(target: "app", "auto delay test task panicked");
            continue;
        };
        if let Some(delay) = best {
            latest.insert(name, delay);
        }
    }

    let measured = latest.len();
    *LATEST_AUTO_DELAY.lock() = latest;
    crate::core::handle::Handle::notify_auto_delay_finished();
    log::info!(
        target: "app",
        "auto delay test finished: {measured} node(s) with results"
    );
    Ok(())
}

struct TestDelayMatchContext {
    group_names: HashSet<String>,
    proxy_names: HashSet<String>,
    proxies: HashMap<String, clash_api::ProxyItemRes>,
    before_connection_ids: HashSet<String>,
}

pub async fn test_delay(url: String) -> Result<TestDelayResult> {
    let test_url = reqwest::Url::parse(&url).ok();
    let test_host = test_url
        .as_ref()
        .and_then(|url| url.host_str())
        .map(|host| host.to_string());
    let test_port = test_url
        .as_ref()
        .and_then(|url| url.port_or_known_default());

    let match_context = prepare_test_delay_match_context().await;

    let mut builder = reqwest::ClientBuilder::new().use_rustls_tls().no_proxy();

    let port = Config::verge()
        .latest()
        .verge_mixed_port
        .unwrap_or(Config::clash().data().get_mixed_port());
    let tun_mode = Config::verge().latest().enable_tun_mode.unwrap_or(false);

    let proxy_scheme = format!("http://127.0.0.1:{port}");

    if !tun_mode {
        if let Ok(proxy) = reqwest::Proxy::http(&proxy_scheme) {
            builder = builder.proxy(proxy);
        }
        if let Ok(proxy) = reqwest::Proxy::https(&proxy_scheme) {
            builder = builder.proxy(proxy);
        }
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_scheme) {
            builder = builder.proxy(proxy);
        }
    }

    let request = builder
        .timeout(Duration::from_millis(10000))
        .build()?
        .get(url).header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0");

    let start = Instant::now();
    let mut request = Box::pin(request.send());
    let mut matched_proxy = None;

    loop {
        tokio::select! {
            response = &mut request => {
                let delay = match response {
                    Ok(response) if response.status().is_success() => start.elapsed().as_millis() as u32,
                    Ok(_) | Err(_) => 10000u32,
                };

                if matched_proxy.is_none() {
                    matched_proxy = match_test_delay_proxy_until(
                        &match_context,
                        test_host.as_deref(),
                        test_port,
                        Duration::from_millis(600),
                    ).await;
                }

                return Ok(TestDelayResult {
                    delay,
                    proxy: matched_proxy,
                });
            }
            _ = sleep(Duration::from_millis(100)), if matched_proxy.is_none() => {
                matched_proxy = match_test_delay_proxy(
                    &match_context,
                    test_host.as_deref(),
                    test_port,
                ).await;
            }
        }
    }
}

async fn prepare_test_delay_match_context() -> TestDelayMatchContext {
    let proxies = clash_api::get_proxies()
        .await
        .map(|res| res.proxies)
        .unwrap_or_default();

    let (group_names, proxy_names) = proxies.iter().fold(
        (HashSet::new(), HashSet::new()),
        |(mut group_names, mut proxy_names), (name, proxy)| {
            if proxy.all.is_some() {
                group_names.insert(name.clone());
            } else {
                proxy_names.insert(name.clone());
            }

            (group_names, proxy_names)
        },
    );

    let before_connection_ids = clash_api::get_connections()
        .await
        .map(|res| {
            res.connections
                .into_iter()
                .map(|connection| connection.id)
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();

    TestDelayMatchContext {
        group_names,
        proxy_names,
        proxies,
        before_connection_ids,
    }
}

async fn match_test_delay_proxy_until(
    context: &TestDelayMatchContext,
    test_host: Option<&str>,
    test_port: Option<u16>,
    timeout: Duration,
) -> Option<String> {
    let deadline = Instant::now() + timeout;

    loop {
        if let Some(proxy) = match_test_delay_proxy(context, test_host, test_port).await {
            return Some(proxy);
        }

        if Instant::now() >= deadline {
            return None;
        }

        sleep(Duration::from_millis(80)).await;
    }
}

async fn match_test_delay_proxy(
    context: &TestDelayMatchContext,
    test_host: Option<&str>,
    test_port: Option<u16>,
) -> Option<String> {
    let connections = clash_api::get_connections().await.ok()?.connections;

    connections
        .into_iter()
        .filter(|connection| !context.before_connection_ids.contains(&connection.id))
        .filter(|connection| connection_matches_test_url(connection, test_host, test_port))
        .find_map(|connection| select_outbound_proxy(&connection, context))
}

fn select_outbound_proxy(
    connection: &clash_api::ConnectionItemRes,
    context: &TestDelayMatchContext,
) -> Option<String> {
    if let Some(proxy) = select_outbound_from_chains(&connection.chains, context) {
        return Some(proxy);
    }

    connection
        .rule
        .as_deref()
        .and_then(normalize_builtin_outbound)
        .or_else(|| {
            connection
                .rule_payload
                .as_deref()
                .and_then(normalize_builtin_outbound)
        })
        .or_else(|| {
            connection
                .chains
                .iter()
                .rev()
                .find_map(|chain| select_builtin_from_policy_hint(Some(chain), context))
        })
}

fn select_outbound_from_chains(
    chains: &[String],
    context: &TestDelayMatchContext,
) -> Option<String> {
    for chain in chains.iter().rev().map(|chain| chain.trim()) {
        if chain.is_empty() {
            continue;
        }

        if let Some(builtin) = normalize_builtin_outbound(chain) {
            return Some(builtin);
        }

        if context.proxy_names.contains(chain) && !context.group_names.contains(chain) {
            return Some(chain.to_string());
        }
    }

    chains
        .iter()
        .rev()
        .map(|chain| chain.trim())
        .find(|chain| !chain.is_empty() && !context.group_names.contains(*chain))
        .map(ToString::to_string)
}

fn select_builtin_from_policy_hint(
    policy_name: Option<&str>,
    context: &TestDelayMatchContext,
) -> Option<String> {
    let policy_name = policy_name?.trim();

    let proxy = context.proxies.get(policy_name)?;
    let selected = proxy
        .now
        .as_deref()
        .or(proxy.selected.as_deref())
        .map(str::trim)?;

    normalize_builtin_outbound(selected)
}

fn normalize_builtin_outbound(name: &str) -> Option<String> {
    if name.eq_ignore_ascii_case("DIRECT") {
        Some("DIRECT".to_string())
    } else if name.eq_ignore_ascii_case("REJECT") {
        Some("REJECT".to_string())
    } else {
        None
    }
}

fn connection_matches_test_url(
    connection: &clash_api::ConnectionItemRes,
    test_host: Option<&str>,
    test_port: Option<u16>,
) -> bool {
    let metadata = &connection.metadata;

    let host_matches = match test_host {
        Some(host) => {
            metadata.host.eq_ignore_ascii_case(host)
                || metadata
                    .host
                    .to_ascii_lowercase()
                    .contains(&host.to_ascii_lowercase())
        }
        None => true,
    };

    let port_matches = match test_port {
        Some(port) => metadata
            .destination_port
            .parse::<u16>()
            .map(|destination_port| destination_port == port)
            .unwrap_or(true),
        None => true,
    };

    host_matches && port_matches
}
