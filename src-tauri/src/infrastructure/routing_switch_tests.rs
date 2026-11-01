use crate::config::{ClashInfo, ConfigSnapshot, IClashSettings, IClashTemp, IProfiles, IRuntime};
use crate::core::clash_api::{self, ConnectionCleanup};
use std::{
    fs,
    process::{Child, Command, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

struct Core(Child);
impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn echo_proxy(name: &'static str) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut stream = BufReader::new(socket);
                let mut line = String::new();
                loop {
                    line.clear();
                    if stream.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    if line == "\r\n" {
                        break;
                    }
                }
                if stream
                    .get_mut()
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .await
                    .is_err()
                {
                    return;
                }
                loop {
                    line.clear();
                    if stream.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    if stream
                        .get_mut()
                        .write_all(format!("{name}:{line}").as_bytes())
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }
    });
    (port, task)
}

async fn tunnel(port: u16, destination: u16) -> BufReader<TcpStream> {
    let mut stream = BufReader::new(TcpStream::connect(("127.0.0.1", port)).await.unwrap());
    stream
        .get_mut()
        .write_all(
            format!(
                "CONNECT 127.0.0.1:{destination} HTTP/1.1\r\nHost: 127.0.0.1:{destination}\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut line = String::new();
    stream.read_line(&mut line).await.unwrap();
    assert!(line.contains("200"), "{line}");
    loop {
        line.clear();
        stream.read_line(&mut line).await.unwrap();
        if line == "\r\n" {
            break;
        }
    }
    stream
}

async fn ping(stream: &mut BufReader<TcpStream>) -> String {
    stream.get_mut().write_all(b"ping\n").await.unwrap();
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_line(&mut response))
        .await
        .unwrap()
        .unwrap();
    response
}

async fn udp_association(port: u16) -> (TcpStream, tokio::net::UdpSocket, std::net::SocketAddr) {
    let mut control = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    control.write_all(&[5, 1, 0]).await.unwrap();
    let mut response = [0; 2];
    control.read_exact(&mut response).await.unwrap();
    assert_eq!(response, [5, 0]);
    control
        .write_all(&[5, 3, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
        .unwrap();
    let mut bound = [0; 10];
    control.read_exact(&mut bound).await.unwrap();
    assert_eq!(&bound[..4], &[5, 0, 0, 1]);
    let relay_port = u16::from_be_bytes([bound[8], bound[9]]);
    (
        control,
        tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap(),
        std::net::SocketAddr::from(([127, 0, 0, 1], relay_port)),
    )
}

#[tokio::test]
async fn production_coordinator_verifies_switch_and_recovers_false_204_once() {
    use crate::{
        application::{configuration, profiles},
        config::{Config, PrfItem},
        core::CoreManager,
        utils::dirs,
    };
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };
    use warp::Filter;
    let (a, proxy_a) = echo_proxy("A").await;
    let (b, proxy_b) = echo_proxy("B").await;
    let reserve = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let core_api = reserve.local_addr().unwrap().port();
    let mixed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mixed_port = mixed.local_addr().unwrap().port();
    let lie = Arc::new(AtomicBool::new(false));
    let unreadable_connections = Arc::new(AtomicBool::new(false));
    let unreadable = unreadable_connections.clone();
    let deletes = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let records = requests.clone();
    let (lying, counter) = (lie.clone(), deletes.clone());
    let gateway = warp::any()
        .and(warp::method())
        .and(warp::path::full())
        .and(warp::body::bytes())
        .and_then(
            move |method: warp::http::Method,
                  path: warp::path::FullPath,
                  body: warp::hyper::body::Bytes| {
                let (lying, counter, records, unreadable) = (
                    lying.clone(),
                    counter.clone(),
                    records.clone(),
                    unreadable.clone(),
                );
                async move {
                    let request_label = format!("{} {}", method, path.as_str());
                    if unreadable.load(Ordering::SeqCst)
                        && method == warp::http::Method::GET
                        && path.as_str() == "/connections"
                    {
                        return Ok(warp::http::Response::builder()
                            .status(503)
                            .body(Vec::new())
                            .unwrap());
                    }
                    if method == warp::http::Method::DELETE
                        && path.as_str().starts_with("/connections/")
                    {
                        counter.fetch_add(1, Ordering::SeqCst);
                        if lying.load(Ordering::SeqCst) {
                            return Ok::<_, std::convert::Infallible>(
                                warp::http::Response::builder()
                                    .status(204)
                                    .body(Vec::new())
                                    .unwrap(),
                            );
                        }
                    }
                    let response = reqwest::Client::builder()
                        .no_proxy()
                        .build()
                        .unwrap()
                        .request(
                            method,
                            format!("http://127.0.0.1:{core_api}{}", path.as_str()),
                        )
                        .bearer_auth("synthetic")
                        .header("Content-Type", "application/json")
                        .body(body)
                        .send()
                        .await;
                    let result = match response {
                        Ok(response) => warp::http::Response::builder()
                            .status(response.status())
                            .body(response.bytes().await.unwrap().to_vec())
                            .unwrap(),
                        Err(_) => warp::http::Response::builder()
                            .status(503)
                            .body(Vec::new())
                            .unwrap(),
                    };
                    records.lock().push(format!(
                        "{request_label} -> {} {}",
                        result.status(),
                        String::from_utf8_lossy(result.body())
                    ));
                    Ok(result)
                }
            },
        );
    let (address, server) = warp::serve(gateway).bind_ephemeral(([127, 0, 0, 1], 0));
    let server = tokio::spawn(server);
    let root = dirs::app_home_dir().unwrap();
    fs::create_dir_all(root.join("profiles")).unwrap();
    fs::create_dir_all(dirs::app_temp_dir().unwrap()).unwrap();
    let config = format!("mixed-port: {mixed_port}\nsocks-port: 0\nport: 0\nexternal-controller: 127.0.0.1:{core_api}\nsecret: synthetic\nmode: rule\nlog-level: warning\nipv6: false\nallow-lan: false\nprofile: {{store-selected: false}}\nproxies:\n  - {{name: A, type: http, server: 127.0.0.1, port: {a}}}\n  - {{name: B, type: http, server: 127.0.0.1, port: {b}}}\nproxy-groups:\n  - {{name: Main, type: select, proxies: [A, B]}}\nrules:\n  - MATCH,Main\n");
    let mut snapshot = ConfigSnapshot {
        revision: 0,
        runtime_revision: 0,
        profile_versions: Default::default(),
        clash: IClashTemp::template(),
        clash_settings: IClashSettings::template(),
        profiles: IProfiles {
            current: Some("synthetic".into()),
            chain: Some(vec![]),
            items: Some(vec![
                PrfItem::global_script(),
                PrfItem {
                    uid: Some("synthetic".into()),
                    itype: Some("local".into()),
                    file: Some("synthetic.yaml".into()),
                    ..Default::default()
                },
            ]),
        },
        runtime: IRuntime::default(),
    };
    snapshot.clash.0 = serde_yaml::from_str(&config).unwrap();
    snapshot.runtime.config = Some(snapshot.clash.0.clone());
    snapshot
        .clash
        .0
        .insert("external-controller".into(), address.to_string().into());
    snapshot.clash_settings.enable_service_mode = Some(false);
    fs::write(root.join("profiles/synthetic.yaml"), &config).unwrap();
    fs::write(
        root.join("profiles")
            .join(crate::config::GLOBAL_SCRIPT_FILE),
        crate::utils::tmpl::ITEM_GLOBAL_SCRIPT,
    )
    .unwrap();
    configuration::initialize_test_snapshot(snapshot.clone());
    drop((reserve, mixed));
    let manager = CoreManager::global();
    struct StopManagedCore;
    impl Drop for StopManagedCore {
        fn drop(&mut self) {
            let _ = std::thread::spawn(|| {
                let _ = CoreManager::global().stop_core();
            })
            .join();
        }
    }
    let _stop = StopManagedCore;
    manager
        .run_core_with_until(
            &snapshot,
            tokio::time::Instant::now() + Duration::from_secs(7),
        )
        .await
        .unwrap();
    let generation = manager.generation();
    let mut old = tunnel(mixed_port, 49001).await;
    assert_eq!(ping(&mut old).await, "A:ping\n");
    profiles::select_proxy("synthetic".into(), "Main".into(), "B".into(), None)
        .await
        .unwrap();
    fs::write(
        root.join("controller-requests.json"),
        serde_json::to_vec_pretty(&*requests.lock()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        generation,
        manager.generation(),
        "normal switches must not restart"
    );
    let mut byte = [0];
    assert_eq!(old.read(&mut byte).await.unwrap(), 0);
    let mut current = tunnel(mixed_port, 49001).await;
    assert_eq!(ping(&mut current).await, "B:ping\n");
    let before_reselect = deletes.load(Ordering::SeqCst);
    profiles::select_proxy("synthetic".into(), "Main".into(), "B".into(), None)
        .await
        .unwrap();
    assert_eq!(
        deletes.load(Ordering::SeqCst),
        before_reselect,
        "reselecting the current node must preserve established sessions"
    );
    assert_eq!(ping(&mut current).await, "B:ping\n");
    lie.store(true, Ordering::SeqCst);
    let started = tokio::time::Instant::now();
    profiles::select_proxy("synthetic".into(), "Main".into(), "A".into(), None)
        .await
        .unwrap();
    assert!(started.elapsed() < Duration::from_secs(7));
    let recovery_ms = started.elapsed().as_millis();
    assert_eq!(
        manager.generation(),
        generation + 1,
        "false 204 requires exactly one restart"
    );
    assert!(deletes.load(Ordering::SeqCst) > 2);
    let closed = tokio::time::timeout(Duration::from_secs(1), current.read(&mut byte))
        .await
        .unwrap();
    assert!(
        matches!(closed, Ok(0))
            || closed.is_err_and(|error| error.kind() == std::io::ErrorKind::ConnectionReset)
    );
    clash_api::verify_selection(Config::clash().get_client_info(), "Main", "A")
        .await
        .unwrap();
    // Mode changes run the same production transaction and keep the recovered PID.
    lie.store(false, Ordering::SeqCst);
    for mode in ["direct", "global", "rule"] {
        configuration::patch_clash(serde_yaml::from_str(&format!("mode: {mode}")).unwrap())
            .await
            .unwrap();
        assert_eq!(
            clash_api::get_configs_for(Config::clash().get_client_info())
                .await
                .unwrap()["mode"],
            mode
        );
    }
    assert_eq!(manager.generation(), generation + 1);
    // A failed pre-change connection snapshot must also recover automatically,
    // not return before the controlled fallback can be attempted.
    unreadable_connections.store(true, Ordering::SeqCst);
    let before_read_failure = manager.generation();
    let started_read_failure = tokio::time::Instant::now();
    profiles::select_proxy("synthetic".into(), "Main".into(), "B".into(), None)
        .await
        .unwrap();
    assert!(started_read_failure.elapsed() < Duration::from_secs(7));
    let pre_read_failure_ms = started_read_failure.elapsed().as_millis();
    assert_eq!(manager.generation(), before_read_failure + 1);
    unreadable_connections.store(false, Ordering::SeqCst);
    clash_api::verify_selection(Config::clash().get_client_info(), "Main", "B")
        .await
        .unwrap();
    // The full production reload path compiles a changed source and keeps the
    // persisted group choice while loading its updated native fields.
    profiles::save_file(
        "synthetic".into(),
        config.replace("log-level: warning", "log-level: info"),
    )
    .await
    .unwrap();
    clash_api::verify_selection(Config::clash().get_client_info(), "Main", "B")
        .await
        .unwrap();
    fs::write(root.join("coordinator-result.json"), serde_json::to_vec_pretty(&serde_json::json!({"normalSwitchNoRestart":true,"false204RecoveryCount":1,"recoveryMs":recovery_ms,"preReadFailureRecoveryMs":pre_read_failure_ms,"modesVerified":["direct","global","rule"],"selectionAfterReload":"B"})).unwrap()).unwrap();
    manager.stop_core_async().await.unwrap();
    server.abort();
    proxy_a.abort();
    proxy_b.abort();
    println!("production coordinator passed; reports={}", root.display());
}

#[tokio::test]
async fn real_core_switch_closes_old_group_tunnels_without_reload_or_restart() {
    use std::os::windows::process::CommandExt;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.tmp/routing-switch")
        .join(nanoid::nanoid!());
    fs::create_dir_all(&root).unwrap();
    let (a, proxy_a) = echo_proxy("A").await;
    let (b, proxy_b) = echo_proxy("B").await;
    let udp_echo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let udp_port = udp_echo.local_addr().unwrap().port();
    let echo_task = tokio::spawn(async move {
        let mut bytes = [0; 512];
        while let Ok((size, peer)) = udp_echo.recv_from(&mut bytes).await {
            let _ = udp_echo.send_to(&bytes[..size], peer).await;
        }
    });
    let mixed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let controller = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mixed_port = mixed.local_addr().unwrap().port();
    let api_port = controller.local_addr().unwrap().port();
    let config = format!("mixed-port: {mixed_port}\nexternal-controller: 127.0.0.1:{api_port}\nsecret: synthetic\nmode: rule\nlog-level: warning\nipv6: false\nallow-lan: false\nprofile: {{store-selected: false}}\nproxies:\n  - {{name: A, type: http, server: 127.0.0.1, port: {a}}}\n  - {{name: B, type: http, server: 127.0.0.1, port: {b}}}\nproxy-groups:\n  - {{name: Main, type: select, proxies: [A, B]}}\n  - {{name: Other, type: select, proxies: [A, B]}}\n  - {{name: Datagram, type: select, proxies: [DIRECT, REJECT]}}\nrules:\n  - DST-PORT,{udp_port},Datagram\n  - DST-PORT,49002,Other\n  - MATCH,Main\n");
    let config_path = root.join("runtime.yaml");
    fs::write(&config_path, &config).unwrap();
    let binary = std::env::var_os("ASTRA_TEST_CORE_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("sidecar/mihomo-x86_64-pc-windows-msvc.exe")
        });
    crate::core::mihomo::verify(&binary).unwrap();
    drop((mixed, controller));
    let log = fs::File::create(root.join("core.log")).unwrap();
    let core = Core(
        Command::new(binary)
            .arg("-d")
            .arg(&root)
            .arg("-f")
            .arg(&config_path)
            .creation_flags(0x08000000)
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap(),
    );
    let pid = core.0.id();
    let info = ClashInfo {
        mixed_port,
        socks_port: 0,
        port: 0,
        server: format!("127.0.0.1:{api_port}"),
        secret: Some("synthetic".into()),
    };
    clash_api::wait_for_core_ready(info.clone(), Duration::from_secs(5))
        .await
        .unwrap();
    let mut old = tunnel(mixed_port, 49001).await;
    let mut unrelated = tunnel(mixed_port, 49002).await;
    assert_eq!(ping(&mut old).await, "A:ping\n");
    assert_eq!(ping(&mut unrelated).await, "A:ping\n");
    let connections = clash_api::get_connections_for(info.clone()).await.unwrap();
    fs::write(
        root.join("connections-before.json"),
        serde_json::to_vec_pretty(&connections).unwrap(),
    )
    .unwrap();
    let cleanup = ConnectionCleanup::capture(&connections, Some(("Main".into(), "A".into())));
    let started = tokio::time::Instant::now();
    clash_api::select_proxy_verified(info.clone(), "Main", "B")
        .await
        .unwrap();
    // A selector update alone intentionally leaves established TCP tunnels on A.
    assert_eq!(ping(&mut old).await, "A:ping\n");
    let mut new = tunnel(mixed_port, 49001).await;
    assert_eq!(ping(&mut new).await, "B:ping\n");
    let closed = cleanup
        .finish(info.clone(), started + Duration::from_secs(4))
        .await;
    fs::write(
        root.join("connections-after.json"),
        serde_json::to_vec_pretty(&clash_api::get_connections_for(info.clone()).await.unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(closed.is_ok(), "{closed:?}; reports={}", root.display());
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), old.read(&mut byte))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    assert_eq!(ping(&mut new).await, "B:ping\n");
    assert_eq!(ping(&mut unrelated).await, "A:ping\n");
    assert_eq!(pid, core.0.id());
    let (_control, udp, relay) = udp_association(mixed_port).await;
    let mut packet = vec![0, 0, 0, 1, 127, 0, 0, 1];
    packet.extend(udp_port.to_be_bytes());
    packet.extend(b"udp-ping");
    udp.send_to(&packet, relay).await.unwrap();
    let mut response = [0; 512];
    let (size, _) = tokio::time::timeout(Duration::from_secs(2), udp.recv_from(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&response[size - 8..size], b"udp-ping");
    let before_udp = clash_api::get_connections_for(info.clone()).await.unwrap();
    assert!(before_udp
        .connections
        .iter()
        .any(|connection| connection.metadata.network == "udp"
            && connection.chains.contains(&"Datagram".into())));
    let udp_cleanup =
        ConnectionCleanup::capture(&before_udp, Some(("Datagram".into(), "DIRECT".into())));
    clash_api::select_proxy_verified(info.clone(), "Datagram", "REJECT")
        .await
        .unwrap();
    udp_cleanup
        .finish(
            info.clone(),
            tokio::time::Instant::now() + Duration::from_secs(1),
        )
        .await
        .unwrap();
    assert!(!clash_api::get_connections_for(info.clone())
        .await
        .unwrap()
        .connections
        .iter()
        .any(|connection| connection.metadata.network == "udp"));
    assert_eq!(ping(&mut unrelated).await, "A:ping\n");
    let mut snapshot = ConfigSnapshot {
        revision: 0,
        runtime_revision: 0,
        profile_versions: Default::default(),
        clash: IClashTemp::template(),
        clash_settings: IClashSettings::template(),
        profiles: IProfiles::template(),
        runtime: IRuntime::default(),
    };
    snapshot.clash.0 = serde_yaml::from_str(&config).unwrap();
    snapshot.runtime.config = Some(snapshot.clash.0.clone());
    let patch = serde_yaml::from_str("mode: global").unwrap();
    clash_api::patch_configs_for(info.clone(), &patch)
        .await
        .unwrap();
    clash_api::select_proxy_verified(info.clone(), "GLOBAL", "Main")
        .await
        .unwrap();
    snapshot
        .runtime
        .config
        .as_mut()
        .unwrap()
        .insert("mode".into(), "global".into());
    clash_api::verify_runtime(&snapshot).await.unwrap();
    // Listener changes are supported live; readback plus TCP readiness is required.
    let reservation = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let replacement = reservation.local_addr().unwrap().port();
    drop(reservation);
    let patch =
        serde_yaml::from_str(&format!("mixed-port: {replacement}\nallow-lan: false")).unwrap();
    clash_api::patch_configs_for(info.clone(), &patch)
        .await
        .unwrap();
    snapshot
        .runtime
        .config
        .as_mut()
        .unwrap()
        .insert("mixed-port".into(), replacement.into());
    clash_api::verify_runtime(&snapshot).await.unwrap();
    let mut live = tunnel(replacement, 49001).await;
    assert_eq!(ping(&mut live).await, "B:ping\n");
    fs::write(root.join("result.json"), serde_json::to_vec_pretty(&serde_json::json!({"corePid":pid,"samePid":true,"newConnectionUses":"B","otherGroupPreserved":true,"oldTunnelClosed":true,"udpSessionRemoved":true,"liveModeAndPortPatch":true,"milliseconds":started.elapsed().as_millis()})).unwrap()).unwrap();
    drop((old, unrelated, new, live, core));
    proxy_a.abort();
    proxy_b.abort();
    echo_task.abort();
    println!("routing switch contract passed; reports={}", root.display());
}
