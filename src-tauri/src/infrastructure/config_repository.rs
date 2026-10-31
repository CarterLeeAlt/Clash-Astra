use crate::config::{ConfigSnapshot, PrfItem, RUNTIME_CONFIG};
use crate::enhance::{
    self,
    chain::{ChainItem, ChainType},
    CompileInput,
};
use crate::utils::{dirs, help};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub type ProfileFiles = BTreeMap<String, Option<String>>;

static WRITE_FAILURE: once_cell::sync::Lazy<parking_lot::Mutex<Option<String>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));

/// 原始配置未能安全留存时，阻止后续事务用默认值覆盖原件。
pub fn block_writes(error: String) {
    log::error!(target: "app", "configuration writes blocked: {error}");
    *WRITE_FAILURE.lock() = Some(error);
}

fn read_profile(item: &PrfItem, files: &ProfileFiles) -> Result<String> {
    let name = item.file.as_ref().context("profile file name is missing")?;
    match files.get(name) {
        Some(Some(data)) => Ok(data.clone()),
        Some(None) => bail!("profile file was removed"),
        None => item.read_file(),
    }
}

fn mapping(data: &str) -> Result<Mapping> {
    let mut value: Value = serde_yaml::from_str(data)?;
    value.apply_merge()?;
    value
        .as_mapping()
        .cloned()
        .context("profile must contain a YAML mapping")
}

pub fn current_mapping(snapshot: &ConfigSnapshot, files: &ProfileFiles) -> Result<Mapping> {
    match &snapshot.profiles.current {
        Some(uid) => mapping(&read_profile(snapshot.profiles.get_item(uid)?, files)?),
        None => Ok(Mapping::new()),
    }
}

fn chain_item(item: &PrfItem, files: &ProfileFiles) -> Result<ChainItem> {
    let source = read_profile(item, files)?;
    let data = match item.itype.as_deref() {
        Some("merge") => ChainType::Merge(mapping(&source)?),
        Some("script") => ChainType::Script(source),
        _ => bail!("enhancement chain only accepts merge and script profiles"),
    };
    Ok(ChainItem {
        uid: item.uid.clone().context("profile UID is missing")?,
        data,
    })
}

pub fn compile(snapshot: &ConfigSnapshot, files: &ProfileFiles) -> Result<crate::config::IRuntime> {
    let profiles = &snapshot.profiles;
    crate::domain::profiles::validate(profiles)?;
    let (current, profile_name) = match profiles.current.as_ref() {
        Some(uid) => {
            let item = profiles.get_item(uid)?;
            if !crate::domain::profiles::is_activatable(item) {
                bail!("current profile is not an activatable subscription");
            }
            (
                mapping(&read_profile(item, files)?)
                    .context("failed to read current subscription")?,
                item.name.clone().unwrap_or_default(),
            )
        }
        None => (Mapping::new(), String::new()),
    };
    crate::domain::network::validate_subscription(&current)?;
    let global_script = profiles
        .items
        .as_ref()
        .and_then(|items| items.iter().find(|item| item.is_global_script()))
        .map(|item| chain_item(item, files))
        .transpose()?;
    let chain = profiles
        .chain
        .iter()
        .flatten()
        .map(|uid| chain_item(profiles.get_item(uid)?, files))
        .collect::<Result<Vec<_>>>()?;
    let runtime = enhance::compile(CompileInput {
        clash_config: snapshot.clash.0.clone(),
        enable_tun: snapshot.verge.enable_tun_mode.unwrap_or(false),
        current,
        profile_name,
        global_script,
        chain,
    });
    if let Some(config) = &runtime.config {
        crate::domain::network::validate_subscription(config)?;
    }
    Ok(runtime)
}

#[derive(Serialize, Deserialize)]
struct FileChange {
    relative: PathBuf,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
pub struct FileTransaction {
    committed: bool,
    entries: Vec<FileChange>,
    #[serde(skip)]
    root: PathBuf,
}

impl FileTransaction {
    pub fn prepare(root: PathBuf, writes: BTreeMap<PathBuf, Option<Vec<u8>>>) -> Result<Self> {
        let mut entries = Vec::new();
        for (relative, after) in writes {
            validate_relative(&relative)?;
            let path = root.join(&relative);
            let before = match fs::read(&path) {
                Ok(data) => Some(data),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
                Err(err) => return Err(err).context("read configuration before transaction"),
            };
            if before != after {
                entries.push(FileChange {
                    relative,
                    before,
                    after,
                });
            }
        }
        let transaction = Self {
            committed: false,
            entries,
            root,
        };
        if transaction.journal_path().exists() {
            bail!("an incomplete configuration recovery must be resolved before another change");
        }
        transaction.write_journal()?;
        Ok(transaction)
    }

    fn journal_path(&self) -> PathBuf {
        self.root.join("runtime/config-transaction.json")
    }

    fn write_journal(&self) -> Result<()> {
        help::write_file_atomic(&self.journal_path(), &serde_json::to_vec(self)?)
    }

    pub fn apply(&self) -> Result<()> {
        for entry in &self.entries {
            write_optional(&self.root.join(&entry.relative), entry.after.as_deref())?;
        }
        Ok(())
    }

    pub fn rollback(&self) -> Result<()> {
        self.restore()?;
        self.finish_rollback()
    }

    pub fn restore(&self) -> Result<()> {
        let mut errors = Vec::new();
        for entry in self.entries.iter().rev() {
            if let Err(err) =
                write_optional(&self.root.join(&entry.relative), entry.before.as_deref())
            {
                errors.push(format!("{}: {err}", entry.relative.display()));
            }
        }
        if !errors.is_empty() {
            bail!("file rollback incomplete: {}", errors.join("; "));
        }
        Ok(())
    }

    pub fn finish_rollback(&self) -> Result<()> {
        remove_journal(&self.journal_path())
    }

    pub fn commit(&mut self) -> Result<()> {
        self.committed = true;
        if let Err(err) = self.write_journal() {
            self.committed = false;
            return Err(err);
        }
        if let Err(err) = remove_journal(&self.journal_path()) {
            log::warn!(target: "app", "committed configuration journal retained for startup cleanup: {err}");
        }
        Ok(())
    }

    pub fn recover(root: PathBuf) -> Result<()> {
        let path = root.join("runtime/config-transaction.json");
        if !path.exists() {
            return Ok(());
        }
        let mut transaction: Self = serde_json::from_slice(&fs::read(&path)?)?;
        transaction.root = root;
        for entry in &transaction.entries {
            validate_relative(&entry.relative)?;
        }
        if transaction.committed {
            remove_journal(&path)
        } else {
            transaction.rollback()
        }
    }
}

fn validate_relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        bail!("invalid relative configuration path");
    }
    Ok(())
}

fn remove_journal(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn write_optional(path: &PathBuf, data: Option<&[u8]>) -> Result<()> {
    match data {
        Some(data) => help::write_file_atomic(path, data),
        None => remove_journal(path),
    }
}

pub fn prepare(snapshot: &ConfigSnapshot, files: &ProfileFiles) -> Result<FileTransaction> {
    if let Some(error) = WRITE_FAILURE.lock().as_ref() {
        bail!("configuration writes are blocked until the original file is repaired and the app restarted: {error}");
    }
    super::profile_catalog::ensure_writable()?;
    prepare_at(dirs::app_home_dir()?, snapshot, files)
}

/// 根目录显式传入，生产与隔离回归共用相同的序列化和文件事务实现。
pub(crate) fn prepare_at(
    root: PathBuf,
    snapshot: &ConfigSnapshot,
    files: &ProfileFiles,
) -> Result<FileTransaction> {
    crate::domain::profiles::validate(&snapshot.profiles)?;
    let mut writes = BTreeMap::new();
    writes.insert(
        "config.yaml".into(),
        Some(serde_yaml::to_string(&snapshot.clash.0)?.into_bytes()),
    );
    writes.insert(
        "verge.yaml".into(),
        Some(serde_yaml::to_string(&snapshot.verge)?.into_bytes()),
    );
    writes.insert(
        "profiles.yaml".into(),
        Some(serde_yaml::to_string(&snapshot.profiles)?.into_bytes()),
    );
    if let Some(runtime) = &snapshot.runtime.config {
        writes.insert(
            RUNTIME_CONFIG.into(),
            // 与启动/重载的序列化保持一致，窗口状态保存不能反复重写运行时文件。
            Some(
                format!(
                    "# Generated by Clash Astra\n\n{}",
                    serde_yaml::to_string(runtime)?
                )
                .into_bytes(),
            ),
        );
    }
    for (name, data) in files {
        help::resolve_profile_path(name)?;
        writes.insert(
            PathBuf::from("profiles").join(name),
            data.as_ref().map(|value| value.as_bytes().to_vec()),
        );
    }
    FileTransaction::prepare(root, writes)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sandbox(PathBuf);
    impl Sandbox {
        fn new() -> Self {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../.tmp/config-transactions")
                .join(nanoid::nanoid!());
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn interrupted_multi_file_change_restores_original_bytes() {
        let sandbox = Sandbox::new();
        fs::write(sandbox.0.join("verge.yaml"), b"old settings").unwrap();
        fs::write(sandbox.0.join("profiles.yaml"), b"old profiles").unwrap();
        let writes = BTreeMap::from([
            (PathBuf::from("verge.yaml"), Some(b"new settings".to_vec())),
            (PathBuf::from("profiles.yaml"), None),
            (
                PathBuf::from("profiles/new.yaml"),
                Some(b"new profile".to_vec()),
            ),
        ]);
        let transaction = FileTransaction::prepare(sandbox.0.clone(), writes).unwrap();
        transaction.apply().unwrap();
        drop(transaction);
        FileTransaction::recover(sandbox.0.clone()).unwrap();
        assert_eq!(
            fs::read(sandbox.0.join("verge.yaml")).unwrap(),
            b"old settings"
        );
        assert_eq!(
            fs::read(sandbox.0.join("profiles.yaml")).unwrap(),
            b"old profiles"
        );
        assert!(!sandbox.0.join("profiles/new.yaml").exists());
    }

    #[test]
    fn committed_journal_cleanup_never_rolls_back_success() {
        let sandbox = Sandbox::new();
        let writes = BTreeMap::from([(PathBuf::from("verge.yaml"), Some(b"new".to_vec()))]);
        let mut transaction = FileTransaction::prepare(sandbox.0.clone(), writes).unwrap();
        transaction.apply().unwrap();
        transaction.committed = true;
        transaction.write_journal().unwrap();
        FileTransaction::recover(sandbox.0.clone()).unwrap();
        assert_eq!(fs::read(sandbox.0.join("verge.yaml")).unwrap(), b"new");
    }

    #[test]
    fn partial_file_write_failure_is_recoverable() {
        let sandbox = Sandbox::new();
        fs::write(sandbox.0.join("a.yaml"), b"before").unwrap();
        fs::write(sandbox.0.join("blocked"), b"not a directory").unwrap();
        let writes = BTreeMap::from([
            (PathBuf::from("a.yaml"), Some(b"after".to_vec())),
            (PathBuf::from("z.yaml"), Some(b"new".to_vec())),
        ]);
        let transaction = FileTransaction::prepare(sandbox.0.clone(), writes).unwrap();
        fs::create_dir(sandbox.0.join("z.yaml")).unwrap();
        assert!(transaction.apply().is_err());
        assert!(transaction.restore().is_err());
        assert!(transaction.journal_path().exists());
        assert_eq!(fs::read(sandbox.0.join("a.yaml")).unwrap(), b"before");
        fs::remove_dir(sandbox.0.join("z.yaml")).unwrap();
        FileTransaction::recover(sandbox.0.clone()).unwrap();
        assert!(!transaction.journal_path().exists());
    }

    #[test]
    fn journal_cannot_write_outside_its_root() {
        for path in ["../outside.yaml", "C:/outside.yaml", ""] {
            assert!(validate_relative(Path::new(path)).is_err());
        }
    }

    #[test]
    fn a_settings_only_commit_does_not_rewrite_runtime_or_subscriptions() {
        let sandbox = Sandbox::new();
        let mut snapshot = ConfigSnapshot {
            revision: 0,
            runtime_revision: 0,
            profile_versions: Default::default(),
            clash: crate::config::IClashTemp::template(),
            verge: crate::config::IVerge::template(),
            profiles: crate::config::IProfiles::template(),
            runtime: crate::config::IRuntime::default(),
        };
        snapshot.runtime.config = Some(snapshot.clash.0.clone());
        let mut first = prepare_at(sandbox.0.clone(), &snapshot, &ProfileFiles::new()).unwrap();
        first.apply().unwrap();
        first.commit().unwrap();
        let runtime_before = fs::read(sandbox.0.join(RUNTIME_CONFIG)).unwrap();
        snapshot.verge.window_is_maximized = Some(true);
        let mut second = prepare_at(sandbox.0.clone(), &snapshot, &ProfileFiles::new()).unwrap();
        assert_eq!(second.entries.len(), 1);
        assert_eq!(second.entries[0].relative, PathBuf::from("verge.yaml"));
        second.apply().unwrap();
        second.commit().unwrap();
        assert_eq!(
            fs::read(sandbox.0.join(RUNTIME_CONFIG)).unwrap(),
            runtime_before
        );
    }

    #[test]
    fn a_sharing_violation_preserves_original_file_and_cleans_staging() {
        use std::os::windows::fs::OpenOptionsExt;
        let sandbox = Sandbox::new();
        let path = sandbox.0.join("locked.yaml");
        fs::write(&path, b"original").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(help::write_file_atomic(&path, b"candidate").is_err());
        drop(held);
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 1);
    }
}
