use crate::config::*;
use crate::core::*;
use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, ClipboardManager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
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

/// 最近一轮站点检测结果（URL -> 结果）。
static LATEST_SITE_TEST: Lazy<Mutex<HashMap<String, TestDelayResult>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// 读取最近一轮自动延迟测试的低值结果。
pub fn latest_auto_delay_results() -> HashMap<String, u32> {
    LATEST_AUTO_DELAY.lock().clone()
}

/// 读取最近一轮站点检测结果。
pub fn latest_site_test_results() -> HashMap<String, TestDelayResult> {
    LATEST_SITE_TEST.lock().clone()
}

/// 同一时刻只允许一轮自动延迟测试，避免启动、定时与订阅切换同时触发。
static AUTO_DELAY_RUNNING: AtomicBool = AtomicBool::new(false);
/// 同一时刻只允许一轮站点检测。
static SITE_TEST_RUNNING: AtomicBool = AtomicBool::new(false);

/// 运行标志的 RAII 守卫，函数退出（含提前返回）时自动复位。
struct RunFlag(&'static AtomicBool);

impl Drop for RunFlag {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn try_acquire(flag: &'static AtomicBool) -> Option<RunFlag> {
    if flag.swap(true, Ordering::SeqCst) {
        None
    } else {
        Some(RunFlag(flag))
    }
}

/// 后台自动延迟测试：逐节点两并发取低值，结果同时写入内核 history
/// 与内存快照（前端窗口重新可见时经 `get_auto_delay_results` 优先展示）。
/// 不依赖前端 WebView，窗口隐藏或被节流时照常执行。
pub async fn run_auto_delay_test() -> Result<()> {
    let Some(_running) = try_acquire(&AUTO_DELAY_RUNNING) else {
        log::debug!(target: "app", "auto delay test skipped; previous round still running");
        return Ok(());
    };

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

/// 站点检测：读取测试页列表，逐 URL 走与手动 Check 相同的测试逻辑，
/// 结果写入快照并推送前端。由测试页的 Test All 触发。
pub async fn run_auto_site_test() -> Result<()> {
    let Some(_running) = try_acquire(&SITE_TEST_RUNNING) else {
        log::debug!(target: "app", "site test skipped; previous round still running");
        return Ok(());
    };

    const CONCURRENCY: usize = 2;

    let urls: Vec<String> = {
        let verge = Config::verge();
        let verge = verge.latest();
        let mut seen = HashSet::new();
        verge
            .test_list
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| item.url)
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
            .filter(|url| seen.insert(url.clone()))
            .collect()
    };

    if urls.is_empty() {
        log::debug!(target: "app", "auto site test skipped; no test urls");
        return Ok(());
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
    let mut set = tokio::task::JoinSet::new();

    for url in urls {
        let semaphore = semaphore.clone();
        set.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("site test semaphore closed");
            let result = test_delay(url.clone()).await.unwrap_or(TestDelayResult {
                delay: 10000,
                proxy: None,
            });
            (url, result)
        });
    }

    let mut latest = HashMap::new();
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((url, result)) => {
                latest.insert(url, result);
            }
            Err(err) => log::warn!(target: "app", "auto site test task failed: {err}"),
        }
    }

    let measured = latest.len();
    let snapshot = latest.clone();
    *LATEST_SITE_TEST.lock() = latest;
    crate::core::handle::Handle::notify_auto_site_test_results(&snapshot);
    log::info!(
        target: "app",
        "auto site test finished: {measured} url(s) with results"
    );
    Ok(())
}

struct TestDelayMatchContext {
    group_names: HashSet<String>,
    proxy_names: HashSet<String>,
    proxies: HashMap<String, clash_api::ProxyItemRes>,
    before_connection_ids: HashSet<String>,
}

/// 站点测试：经当前代理路径与目标网站建立 TCP 连接，并等到目标返回首个字节。
/// 非 TUN 模式通过 mixed-port 的 CONNECT 隧道；TUN 模式直接连接。
/// 不做完整 TLS/HTTP 请求，只测到目标开始响应的耗时。
pub async fn test_delay(url: String) -> Result<TestDelayResult> {
    let mixed_port = Config::verge()
        .latest()
        .verge_mixed_port
        .unwrap_or(Config::clash().data().get_mixed_port());
    let tun_mode = Config::verge().latest().enable_tun_mode.unwrap_or(false);

    test_delay_with(url, mixed_port, tun_mode).await
}

async fn test_delay_with(url: String, mixed_port: u16, tun_mode: bool) -> Result<TestDelayResult> {
    let target = TestTarget::parse(&url)?;
    let match_context = prepare_test_delay_match_context().await;

    let start = Instant::now();
    let probe = async {
        let mut stream = if tun_mode {
            connect_direct(&target.host, target.port).await?
        } else {
            connect_via_http_proxy(mixed_port, &target.host, target.port).await?
        };
        stream.set_nodelay(true).ok();

        if target.scheme == "https" {
            // 代理的 CONNECT 是乐观返回的，只有收到目标响应才能确认 TCP 已建立。
            // 只发 ClientHello，不等完整握手，也不发 HTTP 请求。
            let hello = build_tls_client_hello(&target.host)?;
            stream.write_all(&hello).await?;
        } else {
            let authority = authority(&target.host, target.port);
            let request =
                format!("HEAD / HTTP/1.1\r\nHost: {authority}\r\nConnection: keep-alive\r\n\r\n");
            stream.write_all(request.as_bytes()).await?;
        }

        let mut first = [0u8; 1];
        if stream.read(&mut first).await? == 0 {
            bail!("connection closed before first response byte");
        }

        Ok::<_, anyhow::Error>(stream)
    };

    let (delay, stream) = match tokio::time::timeout(Duration::from_millis(10000), probe).await {
        Ok(Ok(stream)) => (start.elapsed().as_millis() as u32, Some(stream)),
        Ok(Err(err)) => {
            log::debug!(
                target: "app",
                "site connect test failed for {}:{}: {err:#}",
                target.host,
                target.port
            );
            (10000u32, None)
        }
        Err(_) => {
            log::debug!(
                target: "app",
                "site connect test timed out for {}:{}",
                target.host,
                target.port
            );
            (10000u32, None)
        }
    };

    // 隧道保持到出口节点匹配完成；连接关闭后 /connections 里就查不到了
    let proxy = if stream.is_some() {
        match_test_delay_proxy_until(
            &match_context,
            Some(&target.host),
            Some(target.port),
            Duration::from_millis(600),
        )
        .await
    } else {
        None
    };
    drop(stream);

    Ok(TestDelayResult { delay, proxy })
}

struct TestTarget {
    scheme: String,
    host: String,
    port: u16,
}

impl TestTarget {
    fn parse(url: &str) -> Result<Self> {
        let parsed = reqwest::Url::parse(url)
            .map_err(|err| anyhow::anyhow!("invalid test url '{url}': {err}"))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("test url '{url}' has no host"))?
            .to_string();
        let port = parsed
            .port_or_known_default()
            .ok_or_else(|| anyhow::anyhow!("test url '{url}' has no known port"))?;

        Ok(Self {
            scheme: parsed.scheme().to_string(),
            host,
            port,
        })
    }
}

async fn connect_direct(host: &str, port: u16) -> Result<TcpStream> {
    let stream = TcpStream::connect((host, port)).await?;
    stream.set_nodelay(true).ok();
    Ok(stream)
}

/// 通过 mixed-port 发 HTTP CONNECT 建立隧道。
/// mihomo 会先回 200、再在后台连接目标，因此这个函数的返回不代表目标已连通，
/// 真正的连通性由后续写入 ClientHello 后的首个响应字节确认。
async fn connect_via_http_proxy(proxy_port: u16, host: &str, port: u16) -> Result<TcpStream> {
    let mut stream = TcpStream::connect(("127.0.0.1", proxy_port)).await?;
    stream.set_nodelay(true).ok();

    let authority = authority(host, port);
    let request = format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: keep-alive\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await?;

    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    reader.read_line(&mut status).await?;
    if !status.starts_with("HTTP/1.") || !status.contains(" 200") {
        bail!("proxy CONNECT to {authority} rejected: {}", status.trim());
    }
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 || line == "\r\n" {
            break;
        }
    }

    Ok(reader.into_inner())
}

fn authority(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// 生成带 SNI 的 TLS ClientHello；只用于触发目标的真实 TCP 建连与首个响应，
/// 不校验证书、不完成握手，因此空根证书库即可。
fn build_tls_client_hello(host: &str) -> Result<Vec<u8>> {
    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
    let server_name = match host.parse::<IpAddr>() {
        Ok(ip) => rustls::ServerName::IpAddress(ip),
        Err(_) => rustls::ServerName::try_from(host)
            .map_err(|_| anyhow::anyhow!("invalid SNI host '{host}'"))?,
    };
    let mut conn = rustls::ClientConnection::new(Arc::new(config), server_name)?;

    let mut hello = Vec::new();
    loop {
        let written = conn.write_tls(&mut hello)?;
        if written == 0 {
            break;
        }
        if hello.len() > 64 * 1024 {
            bail!("TLS ClientHello exceeds 64 KiB");
        }
    }
    if hello.is_empty() {
        bail!("failed to build TLS ClientHello");
    }
    Ok(hello)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_target() {
        let target = TestTarget::parse("https://www.youtube.com/generate_204").unwrap();
        assert_eq!(target.scheme, "https");
        assert_eq!(target.host, "www.youtube.com");
        assert_eq!(target.port, 443);
    }

    #[test]
    fn parses_explicit_port() {
        let target = TestTarget::parse("http://example.com:8080/path").unwrap();
        assert_eq!(target.scheme, "http");
        assert_eq!(target.port, 8080);
    }

    #[test]
    fn formats_authority_with_ipv6() {
        assert_eq!(authority("example.com", 443), "example.com:443");
        assert_eq!(authority("::1", 443), "[::1]:443");
        assert_eq!(authority("[::1]", 443), "[::1]:443");
    }

    #[test]
    fn builds_tls_client_hello() {
        let hello = build_tls_client_hello("example.com").unwrap();
        assert_eq!(hello.first().copied(), Some(0x16));
        assert!(hello.len() > 5);
    }

    #[tokio::test]
    #[ignore = "requires a running mihomo mixed port on 7897"]
    async fn manual_connect_probe() {
        for url in [
            "https://www.google.com",
            "https://www.youtube.com",
            "https://chatgpt.com",
            "https://github.com",
        ] {
            let result = test_delay_with(url.to_string(), 7897, false).await.unwrap();
            println!("{url} -> {result:?}");
            assert!(result.delay < 10000);
        }
    }
}
