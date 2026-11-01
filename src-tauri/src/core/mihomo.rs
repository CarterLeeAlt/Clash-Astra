use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub use crate::config::mihomo_lock as lock;

/// 在校验或启动前核对已捆绑内核；不自动下载或接受被替换的二进制。
pub fn verify(path: &Path) -> Result<()> {
    let data = fs::read(path).with_context(|| format!("read pinned Mihomo: {}", path.display()))?;
    let actual = format!("{:x}", Sha256::digest(&data));
    anyhow::ensure!(actual == lock().binary_sha256,
        "Mihomo identity mismatch: this application requires {} ({}); reinstall the matching application package",
        lock().version, lock().contract);
    Ok(())
}

pub fn verify_bundled() -> Result<()> {
    #[cfg(test)]
    return verify(&test_binary());
    #[cfg(not(test))]
    verify(&crate::utils::dirs::executable_dir()?.join("mihomo.exe"))
}

pub fn binary_path() -> Result<std::path::PathBuf> {
    #[cfg(test)]
    return Ok(test_binary());
    #[cfg(not(test))]
    Ok(crate::utils::dirs::executable_dir()?.join("mihomo.exe"))
}

pub fn command(name: String) -> Result<tauri::api::process::Command> {
    #[cfg(test)]
    {
        let _ = name;
        Ok(tauri::api::process::Command::new(
            test_binary().to_str().context("test core path")?,
        ))
    }
    #[cfg(not(test))]
    {
        Ok(tauri::api::process::Command::new_sidecar(name)?)
    }
}

#[cfg(test)]
fn test_binary() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("sidecar/mihomo-x86_64-pc-windows-msvc.exe")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_core_matches_and_arbitrary_binary_is_rejected() {
        verify(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("sidecar/mihomo-x86_64-pc-windows-msvc.exe"),
        )
        .unwrap();
        assert!(verify(&std::env::current_exe().unwrap()).is_err());
        assert_eq!(lock().version, "v1.19.33");
    }
}
