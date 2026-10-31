//! 使用生产下载和编译入口，保留生成器的节点、DNS、分组和规则进行联合验收。
use super::{
    config_repository::{self, ProfileFiles},
    mihomo_integration::CoreProcess,
};
use crate::config::*;
use crate::core::clash_api;
use std::os::windows::process::CommandExt;
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

struct SourceServer {
    address: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl SourceServer {
    fn start(body: String) -> Self {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let address = format!("http://{}", socket.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                if let Ok((mut stream, _)) = socket.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut data = [0u8; 16384];
                    let length = stream.read(&mut data).unwrap_or(0);
                    let request = String::from_utf8_lossy(&data[..length]);
                    let response = if request.starts_with("GET /short ") {
                        "HTTP/1.1 302 Found\r\nLocation: /profile\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                    } else {
                        let content = if request.starts_with("GET /empty ") {
                            ""
                        } else if request.starts_with("GET /html ") {
                            "<html>error</html>"
                        } else {
                            body.as_str()
                        };
                        let version = if request.starts_with("GET /mismatch ") {
                            "v0.0.0"
                        } else {
                            "v1.19.33"
                        };
                        format!("HTTP/1.1 200 OK\r\nContent-Type: text/yaml\r\nX-Mihomo-Version: {version}\r\nProfile-Update-Interval: 2\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{content}", content.len())
                    };
                    let _ = stream.write_all(response.as_bytes());
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        });
        Self {
            address,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for SourceServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[tokio::test]
async fn generated_subscription_download_compile_and_complete_load() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let input = std::env::var_os("ASTRA_CONTRACT_SUBSCRIPTION_FILE").map(PathBuf::from);
    let source = input
        .map(|file| fs::read_to_string(file).unwrap())
        .unwrap_or_else(|| include_str!("../../tests/fixtures/sublink-mihomo.yaml").into());
    let server = SourceServer::start(source.clone());
    for route in ["empty", "html", "mismatch"] {
        assert!(
            PrfItem::from_url(&format!("{}/{route}", server.address), None, None, None)
                .await
                .is_err()
        );
    }
    let mut item = PrfItem::from_url(&format!("{}/short", server.address), None, None, None)
        .await
        .expect("production redirect/download/parser path");
    assert_eq!(
        item.option
            .as_ref()
            .and_then(|option| option.update_interval),
        Some(120)
    );
    let downloaded = item.file_data.take().unwrap();
    let original = crate::domain::network::parse_remote_profile(&downloaded).unwrap();
    let root = workspace
        .join(".tmp/mihomo-contract/runs")
        .join(nanoid::nanoid!());
    fs::create_dir_all(&root).unwrap();
    for name in ["geoip.dat", "geosite.dat", "Country.mmdb"] {
        fs::copy(
            workspace.join("src-tauri/resources").join(name),
            root.join(name),
        )
        .unwrap();
    }
    // 预取原 URL 的公开资源只填充缓存，不改生成器的 DNS、规则或 provider 配置。
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap();
    if let Some(providers) = original
        .get("rule-providers")
        .and_then(serde_yaml::Value::as_mapping)
    {
        for provider in providers.values() {
            let Some(url) = provider.get("url").and_then(serde_yaml::Value::as_str) else {
                continue;
            };
            let relative = provider
                .get("path")
                .and_then(serde_yaml::Value::as_str)
                .expect("rule-provider cache path");
            let relative = PathBuf::from(relative.trim_start_matches("./"));
            assert!(
                !relative.is_absolute()
                    && !relative
                        .components()
                        .any(|part| part == std::path::Component::ParentDir)
            );
            let target = root.join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            let bytes = http
                .get(url)
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .bytes()
                .await
                .unwrap();
            assert!(!bytes.is_empty());
            fs::write(target, bytes).unwrap();
        }
    }
    let listeners: Vec<_> = (0..4)
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let ports: Vec<_> = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap().port())
        .collect();
    let uid = item.uid.clone().unwrap();
    let file = item.file.clone().unwrap();
    let mut snapshot = ConfigSnapshot {
        revision: 0,
        runtime_revision: 0,
        profile_versions: Default::default(),
        clash: IClashTemp::template(),
        verge: IVerge::template(),
        profiles: IProfiles::template(),
        runtime: IRuntime::default(),
    };
    snapshot.profiles.items.as_mut().unwrap().push(item);
    snapshot.profiles.current = Some(uid);
    snapshot.clash.0.extend(
        crate::domain::network::PortSettings {
            mixed: ports[0],
            socks: ports[1],
            http: ports[2],
        }
        .mapping(),
    );
    snapshot.clash.0.insert(
        "external-controller".into(),
        format!("127.0.0.1:{}", ports[3]).into(),
    );
    snapshot
        .clash
        .0
        .insert("secret".into(), nanoid::nanoid!().into());
    let files = ProfileFiles::from([
        (file, Some(downloaded)),
        (
            GLOBAL_SCRIPT_FILE.into(),
            Some(crate::utils::tmpl::ITEM_GLOBAL_SCRIPT.into()),
        ),
    ]);
    snapshot.runtime = config_repository::compile(&snapshot, &files).unwrap();
    let runtime = snapshot.runtime.config.as_ref().unwrap();
    for key in [
        "proxies",
        "proxy-groups",
        "proxy-providers",
        "dns",
        "rules",
        "rule-providers",
    ] {
        assert!(
            runtime.get(key) == original.get(key),
            "field fidelity: {key}; values omitted for privacy"
        );
    }
    let path = root.join("runtime.yaml");
    fs::write(&path, serde_yaml::to_string(runtime).unwrap()).unwrap();
    let binary = workspace.join("src-tauri/sidecar/mihomo-x86_64-pc-windows-msvc.exe");
    crate::core::mihomo::verify(&binary).unwrap();
    let validation = Command::new(&binary)
        .args(["-t", "-d"])
        .arg(&root)
        .arg("-f")
        .arg(&path)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    fs::write(root.join("validation.log"), &validation.stdout).unwrap();
    assert!(
        validation.status.success(),
        "locked core rejected the complete config; inspect isolated validation.log"
    );
    drop(listeners);
    let log = fs::File::create(root.join("core.log")).unwrap();
    let core = CoreProcess(
        Command::new(&binary)
            .arg("-d")
            .arg(&root)
            .arg("-f")
            .arg(&path)
            .creation_flags(0x08000000)
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap(),
    );
    clash_api::wait_for_core_ready(snapshot.clash.get_client_info(), Duration::from_secs(12))
        .await
        .unwrap();
    clash_api::wait_subscription_ready(&snapshot).await.unwrap();
    let proxies = clash_api::get_proxies_for(snapshot.clash.get_client_info())
        .await
        .unwrap();
    for node in original
        .get("proxies")
        .and_then(serde_yaml::Value::as_sequence)
        .into_iter()
        .flatten()
    {
        assert!(proxies
            .proxies
            .contains_key(node.get("name").unwrap().as_str().unwrap()));
    }
    if std::env::var("ASTRA_CONTRACT_CONNECTIVITY").as_deref() == Ok("1") {
        let mut failures = 0;
        for (index, node) in original
            .get("proxies")
            .and_then(serde_yaml::Value::as_sequence)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let name = node
                .get("name")
                .and_then(serde_yaml::Value::as_str)
                .unwrap();
            for target in ["https://www.google.com/generate_204", "https://github.com"] {
                let result = clash_api::get_proxy_delay_for(
                    snapshot.clash.get_client_info(),
                    name.to_owned(),
                    Some(target.to_owned()),
                    10000,
                )
                .await;
                let success = result.as_ref().is_ok_and(|value| value.delay < 10000);
                println!(
                    "isolated node connectivity: index={}, target={}, success={}, delay_ms={}",
                    index,
                    target,
                    success,
                    result.as_ref().map_or(10000, |value| value.delay)
                );
                failures += usize::from(!success);
            }
        }
        assert_eq!(
            failures, 0,
            "isolated real-node connectivity failed; details kept only in private test directory"
        );
    }
    drop(core);
    println!(
        "complete subscription contract passed; nodes={}, reports={}",
        original
            .get("proxies")
            .and_then(serde_yaml::Value::as_sequence)
            .map_or(0, Vec::len),
        root.display()
    );
}
