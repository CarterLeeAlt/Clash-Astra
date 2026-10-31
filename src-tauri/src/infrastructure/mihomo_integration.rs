//! 在工作区独立目录运行真实内核，仅验证控制接口，不修改 Windows 系统代理或服务。
use crate::application::transaction::{self, TransactionDriver};
use crate::config::{ConfigSnapshot, IClashTemp, IProfiles, IRuntime, IVerge, PrfItem};
use crate::core::clash_api;
use crate::infrastructure::config_repository::{self, FileTransaction, ProfileFiles};
use std::os::windows::process::CommandExt;
use std::{
    collections::BTreeMap,
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct CoreProcess(Child);
impl Drop for CoreProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn mode(controller: &crate::config::ClashInfo) -> String {
    let config: serde_json::Value = reqwest::ClientBuilder::new()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
        .get(format!("http://{}/configs", controller.server))
        .bearer_auth(controller.secret.as_deref().unwrap_or_default())
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    config["mode"].as_str().unwrap().to_owned()
}

/// 使用真实文件事务和内核接口验证协调流程，唯独发布目标替换为测试布尔值。
struct ReloadOperation {
    files: FileTransaction,
    path: PathBuf,
    controller: crate::config::ClashInfo,
    published: bool,
    fail_commit: bool,
}

impl TransactionDriver for ReloadOperation {
    async fn prepare(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn apply(&mut self) -> anyhow::Result<()> {
        self.files.apply()?;
        clash_api::put_configs_for(self.controller.clone(), self.path.to_str().unwrap()).await
    }
    async fn commit(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.fail_commit, "injected commit failure");
        self.files.commit()
    }
    async fn rollback(&mut self) -> anyhow::Result<()> {
        self.files.restore()?;
        clash_api::put_configs_for(self.controller.clone(), self.path.to_str().unwrap()).await?;
        self.files.finish_rollback()
    }
    fn publish(&mut self) {
        self.published = true;
    }
}

#[tokio::test]
#[ignore = "requires ASTRA_TEST_SUBSCRIPTION_FILE; uses a workspace-local Mihomo process"]
async fn subscription_compile_reload_and_http_rejection() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let input = PathBuf::from(
        std::env::var_os("ASTRA_TEST_SUBSCRIPTION_FILE")
            .expect("subscription fixture path is required"),
    );
    let source = fs::read_to_string(input).expect("read isolated subscription fixture");
    let root = workspace
        .join(".tmp/refactor-integration")
        .join(nanoid::nanoid!());
    fs::create_dir_all(&root).unwrap();
    let binary = workspace.join("src-tauri/sidecar/mihomo-x86_64-pc-windows-msvc.exe");
    assert!(binary.is_file());

    let listeners: Vec<_> = (0..4)
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let ports: Vec<_> = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap().port())
        .collect();
    let mut snapshot = ConfigSnapshot {
        revision: 0,
        runtime_revision: 0,
        profile_versions: Default::default(),
        clash: IClashTemp::template(),
        verge: IVerge::template(),
        runtime: IRuntime::default(),
        profiles: IProfiles {
            current: Some("fixture".into()),
            chain: Some(vec![]),
            items: Some(vec![
                PrfItem::global_script(),
                PrfItem {
                    uid: Some("fixture".into()),
                    itype: Some("local".into()),
                    file: Some("fixture.yaml".into()),
                    ..Default::default()
                },
            ]),
        },
    };
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
        ("fixture.yaml".into(), Some(source)),
        (
            crate::config::GLOBAL_SCRIPT_FILE.into(),
            Some(crate::utils::tmpl::ITEM_GLOBAL_SCRIPT.into()),
        ),
    ]);
    snapshot.runtime =
        config_repository::compile(&snapshot, &files).expect("compile subscription fixture");
    let runtime = snapshot.runtime.config.as_mut().unwrap();
    let node_count = runtime
        .get("proxies")
        .and_then(|value| value.as_sequence())
        .map_or(0, Vec::len);
    assert!(node_count > 0 || runtime.contains_key("proxy-providers"));
    // 控制接口回归保留订阅节点和分组，固定本地路由，避免测试依赖远程规则库更新。
    runtime.insert("rules".into(), vec!["MATCH,DIRECT"].into());
    runtime.insert("rule-providers".into(), serde_yaml::Mapping::new().into());
    runtime.insert(
        "dns".into(),
        serde_yaml::from_str::<serde_yaml::Value>("enable: false").unwrap(),
    );
    let path = root.join("runtime.yaml");
    fs::write(&path, serde_yaml::to_string(runtime).unwrap()).unwrap();
    let output = Command::new(&binary)
        .args(["-t", "-d"])
        .arg(&root)
        .arg("-f")
        .arg(&path)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    fs::write(root.join("validation.log"), &output.stdout).unwrap();
    assert!(
        output.status.success(),
        "Mihomo validation failed; inspect isolated validation.log"
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
    let info = snapshot.clash.get_client_info();
    clash_api::wait_for_core_ready(info.clone(), Duration::from_secs(12))
        .await
        .unwrap();
    let proxies = clash_api::get_proxies_for(info.clone()).await.unwrap();
    assert!(!proxies.proxies.is_empty());
    let patch = serde_yaml::from_str("mode: direct").unwrap();
    clash_api::patch_configs_for(info.clone(), &patch)
        .await
        .unwrap();
    let invalid_auth = crate::config::ClashInfo {
        secret: Some("incorrect-test-token".into()),
        ..info.clone()
    };
    assert!(clash_api::patch_configs_for(invalid_auth, &patch)
        .await
        .is_err());
    assert_eq!(mode(&info).await, "direct");
    clash_api::patch_configs_for(info.clone(), &serde_yaml::from_str("mode: rule").unwrap())
        .await
        .unwrap();

    let original = fs::read(&path).unwrap();
    let mut operation = ReloadOperation {
        files: FileTransaction::prepare(
            root.clone(),
            BTreeMap::from([(
                PathBuf::from("runtime.yaml"),
                Some(b"proxies:\n  - name: broken\n    type: unsupported-protocol\n".to_vec()),
            )]),
        )
        .unwrap(),
        path: path.clone(),
        controller: info.clone(),
        published: false,
        fail_commit: false,
    };
    assert!(transaction::execute(&mut operation).await.is_err());
    assert!(!operation.published);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(mode(&info).await, "rule");
    assert!(!root.join("runtime/config-transaction.json").exists());
    assert!(!clash_api::get_proxies_for(info.clone())
        .await
        .unwrap()
        .proxies
        .is_empty());

    // 内核接受新配置后提交失败，也必须恢复文件与实际内核状态。
    let mut candidate = snapshot.runtime.config.clone().unwrap();
    candidate.insert("mode".into(), "global".into());
    let mut operation = ReloadOperation {
        files: FileTransaction::prepare(
            root.clone(),
            BTreeMap::from([(
                PathBuf::from("runtime.yaml"),
                Some(serde_yaml::to_string(&candidate).unwrap().into_bytes()),
            )]),
        )
        .unwrap(),
        path: path.clone(),
        controller: info.clone(),
        published: false,
        fail_commit: true,
    };
    assert!(transaction::execute(&mut operation).await.is_err());
    assert!(!operation.published);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(mode(&info).await, "rule");
    assert!(!root.join("runtime/config-transaction.json").exists());

    let mut file_transaction = FileTransaction::prepare(
        root.clone(),
        BTreeMap::from([(
            PathBuf::from("runtime.yaml"),
            Some(
                serde_yaml::to_string(&snapshot.runtime.config)
                    .unwrap()
                    .into_bytes(),
            ),
        )]),
    )
    .unwrap();
    file_transaction.apply().unwrap();
    clash_api::put_configs_for(info, path.to_str().unwrap())
        .await
        .unwrap();
    file_transaction.commit().unwrap();
    drop(core);
    println!(
        "isolated Mihomo integration passed: subscription_nodes={node_count}, reports={}",
        root.display()
    );
}
